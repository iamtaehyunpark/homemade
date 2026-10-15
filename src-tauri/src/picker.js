// Homemade element picker — injected into every *frame* of a Homemade window (not the top frame, which is Homemade's own
// UI). Inside a program's web UI it behaves like a browser devtools element cursor: select mode highlights the element
// under the pointer, a click describes it (tag, id/classes/attrs, text, CSS path, React/Vue component + source file when
// available) and posts it to Homemade, which types the mention into the agent session. Protocol (postMessage):
//   Homemade → frame  {homemade:"select", on:boolean}
//   frame → Homemade  {homemade:"picker-ready"} | {homemade:"hover"} | {homemade:"pick", desc} | {homemade:"select-cancel"}
(() => {
  if (window.top === window || window.__homemadePicker) return;
  window.__homemadePicker = true;
  const top = window.top;
  let on = false, box = null, tip = null;
  const clean = (s) => (s || "").replace(/\s+/g, " ").trim();

  const reactInfo = (el) => {
    const key = Object.keys(el).find((k) => k.startsWith("__reactFiber$"));
    if (!key) return {};
    let f = el[key], comps = [], src = null;
    while (f && comps.length < 3) {
      const t = f.type;
      if (typeof t === "function" || (t && typeof t === "object" && (t.displayName || t.render))) {
        const name = t.displayName || t.name || (t.render && t.render.name);
        if (name && name !== "Fragment") comps.push(name);
      }
      if (!src && f._debugSource) src = f._debugSource.fileName + ":" + f._debugSource.lineNumber;
      f = f.return;
    }
    return { component: comps.join(" < ") || undefined, source: src || undefined };
  };
  const vueInfo = (el) => {
    const c = el.__vueParentComponent || el.__vue__;
    if (!c) return {};
    const name = (c.type && (c.type.name || c.type.__name)) || (c.$options && c.$options.name);
    const file = c.type && c.type.__file;
    return name ? { component: name, source: file || undefined } : {};
  };
  const cssPath = (el) => {
    const parts = [];
    let n = el;
    while (n && n.nodeType === 1 && parts.length < 6 && n !== document.body) {
      if (n.id) { parts.unshift("#" + CSS.escape(n.id)); break; }
      const tid = n.getAttribute("data-testid");
      if (tid) { parts.unshift(`[data-testid="${tid}"]`); break; }
      let s = n.tagName.toLowerCase();
      const sib = n.parentElement ? Array.from(n.parentElement.children).filter((c) => c.tagName === n.tagName) : [];
      if (sib.length > 1) s += `:nth-of-type(${sib.indexOf(n) + 1})`;
      parts.unshift(s);
      n = n.parentElement;
    }
    return parts.join(" > ");
  };
  const describe = (el) => {
    const tag = el.tagName.toLowerCase();
    const attrs = [];
    if (el.id) attrs.push("#" + el.id);
    const cls = typeof el.className === "string" ? el.className.split(/\s+/).filter(Boolean).slice(0, 3) : [];
    if (cls.length) attrs.push("." + cls.join("."));
    for (const a of ["data-testid", "name", "aria-label", "placeholder", "type", "role", "href"]) {
      const v = el.getAttribute(a);
      if (v) attrs.push(`[${a}="${v.slice(0, 60)}"]`);
    }
    const isField = /^(input|textarea|select)$/.test(tag);
    let text = clean(isField ? el.value : (el.innerText || el.textContent));
    if (text.length > 90) text = text.slice(0, 89) + "…";
    const r = el.getBoundingClientRect();
    return { tag, attrs, text, path: cssPath(el), ...vueInfo(el), ...reactInfo(el), page: location.pathname + location.search, title: document.title,
      rect: { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) } };
  };
  const label = (d) => `${d.tag}${d.attrs.join("")}${d.text ? ` "${d.text}"` : ""}${d.component ? ` ‹${d.component}›` : ""}`;

  const ensure = () => {
    if (box) return;
    box = document.createElement("div");
    box.style.cssText = "position:fixed;z-index:2147483647;pointer-events:none;border:2px solid #3b82f6;background:rgba(59,130,246,.12);border-radius:3px;transition:all 60ms;box-sizing:border-box";
    tip = document.createElement("div");
    tip.style.cssText = "position:fixed;z-index:2147483647;pointer-events:none;max-width:70vw;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;background:#111;color:#fff;font:11px ui-monospace,Menlo,monospace;padding:2px 6px;border-radius:4px;box-shadow:0 1px 4px rgba(0,0,0,.3)";
    document.documentElement.append(box, tip);
  };
  const clear = () => { if (box) box.remove(); if (tip) tip.remove(); box = tip = null; };
  const under = (x, y) => document.elementsFromPoint(x, y).find((e) => e !== box && e !== tip) || null;
  const move = (e) => {
    const el = under(e.clientX, e.clientY);
    if (!el) return;
    ensure();
    const d = describe(el);
    Object.assign(box.style, { left: d.rect.x + "px", top: d.rect.y + "px", width: d.rect.w + "px", height: d.rect.h + "px", display: "block" });
    tip.textContent = label(d);
    tip.style.left = Math.max(0, Math.min(d.rect.x, innerWidth - 420)) + "px";
    tip.style.top = (d.rect.y > 26 ? d.rect.y - 22 : d.rect.y + d.rect.h + 4) + "px";
    tip.style.display = "block";
    top.postMessage({ homemade: "hover" }, "*");
  };
  const click = (e) => {
    e.preventDefault(); e.stopPropagation();
    const el = under(e.clientX, e.clientY);
    if (el) top.postMessage({ homemade: "pick", desc: describe(el) }, "*");
  };
  const swallow = (e) => { e.preventDefault(); e.stopPropagation(); };
  const key = (e) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); top.postMessage({ homemade: "select-cancel" }, "*"); } };
  const set = (next) => {
    if (next === on) return;
    on = next;
    const m = on ? "addEventListener" : "removeEventListener";
    document[m]("mousemove", move, true);
    document[m]("click", click, true);
    document[m]("mousedown", swallow, true);
    document[m]("mouseup", swallow, true);
    document[m]("keydown", key, true);
    document.documentElement.style.cursor = on ? "crosshair" : "";
    if (!on) clear();
  };
  window.addEventListener("message", (e) => {
    const d = e.data;
    if (!d || d.homemade !== "select") return;
    set(!!d.on);
  });
  const ready = () => top.postMessage({ homemade: "picker-ready" }, "*");
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", ready); else ready();
})();
