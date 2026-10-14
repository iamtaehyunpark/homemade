"""Static CLI inference for Homemade. Zero deps. Never imports/executes the target.
Usage: python3 -I ast_inspector.py <script.py>
Prints {"inputs": {...}, "entry": "<path>", "framework": "argparse"|"typer"|"click"|"none"}.
"""
import ast
import json
import sys
from pathlib import Path

FILEISH = ("file", "path", "input", "img", "image", "video", "audio", "src", "source")
DIRISH = ("dir", "folder")


def _const(node):
    if isinstance(node, ast.Constant):
        return node.value
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub) and isinstance(node.operand, ast.Constant):
        v = node.operand.value
        if isinstance(v, (int, float)):
            return -v
    if isinstance(node, (ast.List, ast.Tuple)):
        vals = [_const(e) for e in node.elts]
        return vals
    return None


def _name(node):
    """Dotted name of an expression: argparse.FileType -> 'argparse.FileType'."""
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        base = _name(node.value)
        return f"{base}.{node.attr}" if base else node.attr
    if isinstance(node, ast.Call):
        return _name(node.func)
    if isinstance(node, ast.Subscript):
        return _name(node.value)
    return None


def _label(key):
    return key.replace("_", " ").strip().title()


def _type_from_name(n):
    if not n:
        return None
    last = n.split(".")[-1]
    return {
        "int": "int", "float": "float", "bool": "bool", "str": "string",
        "Path": "file", "PurePath": "file", "FileType": "file", "File": "file",
    }.get(last)


def _semantic(spec, key):
    if spec["type"] == "string":
        k = key.lower()
        if any(w in k for w in DIRISH):
            spec["type"] = "directory"
        elif any(w in k for w in FILEISH):
            spec["type"] = "file"


def _sorted_calls(tree):
    calls = [n for n in ast.walk(tree) if isinstance(n, ast.Call)]
    calls.sort(key=lambda n: (getattr(n, "lineno", 0), getattr(n, "col_offset", 0)))
    return calls


# ---------------- argparse ----------------
def infer_argparse(tree):
    inputs = {}
    positional_count = 0
    for node in _sorted_calls(tree):
        if not (isinstance(node.func, ast.Attribute) and node.func.attr == "add_argument"):
            continue
        flags = [a.value for a in node.args if isinstance(a, ast.Constant) and isinstance(a.value, str)]
        if not flags:
            continue
        long_flags = [f for f in flags if f.startswith("--")]
        primary = max(long_flags or flags, key=len)
        is_positional = not primary.startswith("-")
        key = primary.lstrip("-").replace("-", "_")
        if key == "help":
            continue
        spec = {
            "type": "string",
            "cli_flag": None if is_positional else primary,
            "positional_index": positional_count if is_positional else None,
            "label": _label(key),
        }
        if is_positional:
            positional_count += 1
            spec["required"] = True
        nargs = None
        for kw in node.keywords:
            if kw.arg == "dest" and isinstance(kw.value, ast.Constant):
                pass  # keep key derived from flag; dest only affects the script side
            elif kw.arg == "type":
                t = _type_from_name(_name(kw.value))
                if t:
                    spec["type"] = t
            elif kw.arg == "action" and isinstance(kw.value, ast.Constant):
                if kw.value.value in ("store_true", "store_false"):
                    spec["type"] = "bool"
                    spec["bool_style"] = "flag_only"
                    spec["default"] = kw.value.value == "store_false"
            elif kw.arg == "action" and _name(kw.value) and _name(kw.value).endswith("BooleanOptionalAction"):
                spec["type"] = "bool"
                spec["bool_style"] = "flag_only"
            elif kw.arg == "default":
                v = _const(kw.value)
                if v is not None:
                    spec["default"] = v
            elif kw.arg == "choices" and isinstance(kw.value, (ast.List, ast.Tuple)):
                opts = [e.value for e in kw.value.elts if isinstance(e, ast.Constant)]
                spec["type"] = "select"
                spec["options"] = [str(o) for o in opts]
            elif kw.arg == "required" and isinstance(kw.value, ast.Constant):
                spec["required"] = bool(kw.value.value)
            elif kw.arg == "help" and isinstance(kw.value, ast.Constant) and isinstance(kw.value.value, str):
                spec["description"] = kw.value.value
            elif kw.arg == "nargs" and isinstance(kw.value, ast.Constant):
                nargs = kw.value.value
        if nargs in ("+", "*") and spec["type"] in ("string", "file"):
            k = key.lower()
            if spec["type"] == "file" or any(w in k for w in FILEISH) or k.endswith("s"):
                spec["type"] = "file_list"
        if nargs == "?" and is_positional:
            spec["required"] = False
        _semantic(spec, key)
        inputs[key] = spec
    return inputs


# ---------------- click ----------------
def _is_click(dec, attr):
    return isinstance(dec, ast.Call) and _name(dec.func) in (f"click.{attr}", attr)


def infer_click(tree):
    inputs = {}
    found = False
    for fn in ast.walk(tree):
        if not isinstance(fn, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        if not any(_is_click(d, a) or _name(d) in ("click.command", "click.group") for d in fn.decorator_list for a in ("command", "group")):
            continue
        found = True
        pos = 0
        # decorators apply bottom-up but click shows params top-down; use source order
        for dec in fn.decorator_list:
            if _is_click(dec, "argument"):
                names = [a.value for a in dec.args if isinstance(a, ast.Constant) and isinstance(a.value, str)]
                if not names:
                    continue
                key = names[0].replace("-", "_")
                spec = {"type": "string", "cli_flag": None, "positional_index": pos, "label": _label(key), "required": True}
                pos += 1
                _click_kw(spec, dec, key)
                inputs[key] = spec
            elif _is_click(dec, "option"):
                names = [a.value for a in dec.args if isinstance(a, ast.Constant) and isinstance(a.value, str)]
                flags = [n.split("/")[0] for n in names if n.startswith("-")]
                if not flags:
                    continue
                long_flags = [f for f in flags if f.startswith("--")]
                primary = max(long_flags or flags, key=len)
                plain = [n for n in names if not n.startswith("-")]
                key = (plain[0] if plain else primary.lstrip("-")).replace("-", "_")
                spec = {"type": "string", "cli_flag": primary, "positional_index": None, "label": _label(key)}
                _click_kw(spec, dec, key)
                inputs[key] = spec
    return inputs, found


def _click_kw(spec, dec, key):
    multiple = False
    for kw in dec.keywords:
        if kw.arg == "type":
            n = _name(kw.value)
            if n and n.split(".")[-1] == "Choice" and isinstance(kw.value, ast.Call) and kw.value.args:
                opts = _const(kw.value.args[0]) or []
                spec["type"] = "select"
                spec["options"] = [str(o) for o in opts if o is not None]
            elif n and n.split(".")[-1] == "Path":
                spec["type"] = "file"
                for k2 in getattr(kw.value, "keywords", []):
                    if k2.arg == "file_okay" and isinstance(k2.value, ast.Constant) and k2.value.value is False:
                        spec["type"] = "directory"
            else:
                t = _type_from_name(n)
                if t:
                    spec["type"] = t
        elif kw.arg == "is_flag" and isinstance(kw.value, ast.Constant) and kw.value.value:
            spec["type"] = "bool"
            spec["bool_style"] = "flag_only"
            spec.setdefault("default", False)
        elif kw.arg == "default":
            v = _const(kw.value)
            if v is not None:
                spec["default"] = v
        elif kw.arg == "required" and isinstance(kw.value, ast.Constant):
            spec["required"] = bool(kw.value.value)
        elif kw.arg == "help" and isinstance(kw.value, ast.Constant) and isinstance(kw.value.value, str):
            spec["description"] = kw.value.value
        elif kw.arg in ("multiple",) and isinstance(kw.value, ast.Constant) and kw.value.value:
            multiple = True
        elif kw.arg == "nargs" and isinstance(kw.value, ast.Constant) and kw.value.value == -1:
            multiple = True
    if multiple and spec["type"] in ("string", "file"):
        spec["type"] = "file_list"
    _semantic(spec, key)


# ---------------- typer ----------------
def infer_typer(tree):
    uses_typer = any(
        (isinstance(n, ast.Import) and any(a.name == "typer" for a in n.names))
        or (isinstance(n, ast.ImportFrom) and n.module == "typer")
        for n in ast.walk(tree)
    )
    if not uses_typer:
        return {}, False
    target = None
    funcs = {f.name: f for f in ast.walk(tree) if isinstance(f, (ast.FunctionDef, ast.AsyncFunctionDef))}
    # typer.run(main)
    for c in _sorted_calls(tree):
        if _name(c.func) in ("typer.run", "run") and c.args and isinstance(c.args[0], ast.Name) and c.args[0].id in funcs:
            target = funcs[c.args[0].id]
            break
    if target is None:
        for f in funcs.values():
            if any(_name(d) and _name(d).endswith(".command") for d in f.decorator_list):
                target = f
                break
    if target is None:
        target = funcs.get("main")
    if target is None:
        return {}, True
    inputs = {}
    args = target.args.args
    defaults = [None] * (len(args) - len(target.args.defaults)) + list(target.args.defaults)
    pos = 0
    for a, d in zip(args, defaults):
        key = a.arg
        ann = a.annotation
        # Annotated[T, typer.Option(...)]
        marker = None
        if isinstance(ann, ast.Subscript) and _name(ann.value) in ("Annotated", "typing.Annotated", "typing_extensions.Annotated"):
            sl = ann.slice
            elts = sl.elts if isinstance(sl, ast.Tuple) else [sl]
            ann = elts[0]
            for e in elts[1:]:
                if isinstance(e, ast.Call) and _name(e.func) and _name(e.func).split(".")[-1] in ("Option", "Argument"):
                    marker = e
        if isinstance(d, ast.Call) and _name(d.func) and _name(d.func).split(".")[-1] in ("Option", "Argument"):
            marker = d
            d = None
        kind = _name(marker.func).split(".")[-1] if marker is not None else ("Argument" if d is None else "Option")
        ftype = "string"
        tn = _name(ann) if ann is not None else None
        if isinstance(ann, ast.Subscript) and _name(ann.value) and _name(ann.value).split(".")[-1] in ("List", "list"):
            ftype = "file_list"
        elif isinstance(ann, ast.Subscript) and _name(ann.value) and _name(ann.value).split(".")[-1] == "Optional":
            ftype = _type_from_name(_name(ann.slice)) or "string"
        else:
            ftype = _type_from_name(tn) or "string"
        if kind == "Argument":
            spec = {"type": ftype, "cli_flag": None, "positional_index": pos, "label": _label(key), "required": True}
            pos += 1
        else:
            flag = "--" + key.replace("_", "-")
            if marker is not None:
                fl = [x.value for x in marker.args if isinstance(x, ast.Constant) and isinstance(x.value, str) and x.value.startswith("-")]
                lf = [x for x in fl if x.startswith("--")]
                if lf or fl:
                    flag = max(lf or fl, key=len).split("/")[0]
            spec = {"type": ftype, "cli_flag": flag, "positional_index": None, "label": _label(key)}
            if ftype == "bool":
                spec["bool_style"] = "flag_only"
        default_node = d
        if marker is not None:
            if marker.args and not (isinstance(marker.args[0], ast.Constant) and isinstance(marker.args[0].value, str) and marker.args[0].value.startswith("-")):
                default_node = marker.args[0]
            for kw in marker.keywords:
                if kw.arg == "default":
                    default_node = kw.value
                elif kw.arg == "help" and isinstance(kw.value, ast.Constant) and isinstance(kw.value.value, str):
                    spec["description"] = kw.value.value
        if default_node is not None:
            if isinstance(default_node, ast.Constant) and default_node.value is Ellipsis:
                spec["required"] = True
            else:
                v = _const(default_node)
                if v is not None:
                    spec["default"] = v
                    if kind == "Argument":
                        spec["required"] = False
        _semantic(spec, key)
        inputs[key] = spec
    return inputs, True


def main():
    if len(sys.argv) < 2:
        print(json.dumps({"inputs": {}, "framework": "none", "error": "usage: ast_inspector.py <script>"}))
        return
    path = Path(sys.argv[1])
    try:
        source = path.read_text(encoding="utf-8", errors="replace")
        tree = ast.parse(source, filename=str(path))
    except Exception as e:  # SyntaxError, IO errors
        print(json.dumps({"inputs": {}, "entry": str(path), "framework": "none", "error": f"{type(e).__name__}: {e}"}))
        return
    try:
        inputs = infer_argparse(tree)
        framework = "argparse" if inputs else "none"
        if not inputs:
            ti, used = infer_typer(tree)
            ci, cused = infer_click(tree)
            if ci or (cused and not ti):
                inputs, framework = ci, "click"
            elif used:
                inputs, framework = ti, "typer"
        print(json.dumps({"inputs": inputs, "entry": str(path), "framework": framework}))
    except Exception as e:
        print(json.dumps({"inputs": {}, "entry": str(path), "framework": "none", "error": f"{type(e).__name__}: {e}"}))


if __name__ == "__main__":
    main()
