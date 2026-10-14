"""Homemade warm RPC worker + directive helpers. Zero dependencies; runs under `python3 -I`.

Usage:
    from homemade_worker import expose, run_worker, progress, artifact, metric

    @expose
    def apply_filter(image_path: str, sigma: float): ...

    if __name__ == "__main__":
        run_worker()
"""
import json
import sys
import traceback

__all__ = ["expose", "run_worker", "progress", "artifact", "metric"]

_REGISTRY = {}
# Keep a handle on the real stdout so user prints can't corrupt the protocol framing.
_OUT = sys.stdout


def _emit(line):
    _OUT.write(line + "\n")
    _OUT.flush()


def expose(fn):
    """Register a function callable via window.homemade.call(name, params)."""
    _REGISTRY[fn.__name__] = fn
    return fn


def progress(pct, msg=""):
    _emit("@homemade:progress %d %s" % (int(pct), json.dumps(str(msg))))


def artifact(path):
    _emit("@homemade:artifact %s" % json.dumps(str(path)))


def metric(data):
    _emit("@homemade:metric %s" % json.dumps(data, default=str))


def _respond(obj):
    _emit("@homemade:rpc_response %s" % json.dumps(obj, default=str))


def run_worker():
    _emit("@homemade:worker_ready")
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        req_id = None
        try:
            req = json.loads(line)
            req_id = req.get("id")
            method = req.get("method")
            if method == "__shutdown__":
                break
            params = req.get("params") or {}
            fn = _REGISTRY.get(method)
            if fn is None:
                raise KeyError("Unknown method: %s (exposed: %s)" % (method, ", ".join(sorted(_REGISTRY))))
            res = fn(**params) if isinstance(params, dict) else fn(*params)
            out = {"id": req_id, "ok": True, "result": res}
        except Exception as e:  # noqa: BLE001
            out = {"id": req_id, "ok": False, "error": str(e), "trace": traceback.format_exc()}
        _respond(out)
    sys.exit(0)
