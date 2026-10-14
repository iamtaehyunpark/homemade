/* Homemade iframe bridge: exposes window.homemade inside sandboxed app UIs.
 * Talks to the host (AppSandbox) exclusively via postMessage. Plain ES2020, no build step. */
(function () {
  "use strict";
  if (window.homemade && window.homemade.__bridge) return;

  var TERMINAL = { success: 1, succeeded: 1, failed: 1, killed: 1, cancelled: 1, canceled: 1, error: 1, oom: 1, timeout: 1, timed_out: 1, completed: 1, done: 1 };
  var host = window.parent;
  var nextId = 1;
  var pending = new Map();          // rpc id -> {resolve, reject}
  var runWaiters = new Map();       // runId -> {resolve, reject}
  var finishedRuns = new Map();     // runId -> final status payload (status arriving before rpc_result)
  var listeners = new Map();        // event name -> Set<cb>
  var info = { appId: null, schema: null, platform: null };
  var e2eEnabled = false;
  var resolveReady;
  var ready = new Promise(function (r) { resolveReady = r; });

  function post(msg) {
    msg.homemade = 1;
    host.postMessage(msg, "*");
  }

  function rpc(method, params) {
    var id = "r" + nextId++;
    return new Promise(function (resolve, reject) {
      pending.set(id, { resolve: resolve, reject: reject });
      post({ type: "rpc", id: id, method: method, params: params === undefined ? {} : params });
    });
  }

  function on(name, cb) {
    if (typeof cb !== "function") throw new TypeError("callback must be a function");
    if (!listeners.has(name)) listeners.set(name, new Set());
    listeners.get(name).add(cb);
    return function () { var s = listeners.get(name); if (s) s.delete(cb); };
  }

  function emit(name, payload) {
    var s = listeners.get(name);
    if (!s) return;
    s.forEach(function (cb) {
      try { cb(payload); } catch (e) { console.error("[homemade] listener error", e); }
    });
  }

  function statusOf(p) {
    if (!p) return null;
    return p.status || (p.run && p.run.status) || null;
  }
  function runIdOf(p) {
    if (!p) return null;
    return p.runId || p.run_id || p.id || (p.run && (p.run.id || p.run.runId)) || null;
  }

  function settleRun(runId, payload) {
    var w = runWaiters.get(runId);
    var record = (payload && payload.run) || payload;
    if (w) { runWaiters.delete(runId); w.resolve(record); }
    else { finishedRuns.set(runId, record); }
  }

  window.addEventListener("message", function (ev) {
    var m = ev.data;
    if (!m || m.homemade !== 1 || ev.source !== host) return;
    switch (m.type) {
      case "init":
        info.appId = m.appId; info.schema = m.schema; info.platform = m.platform;
        e2eEnabled = m.e2e === true;
        resolveReady(api);
        break;
      case "e2e_eval": {
        if (!e2eEnabled) return;
        var eid = m.id;
        var reply = function (ok, val) {
          var msg = { type: "e2e_result", id: eid, ok: ok };
          if (ok) {
            try { msg.result = val === undefined ? null : JSON.parse(JSON.stringify(val)); } catch (_) { msg.result = String(val); }
          } else msg.error = val && val.message ? String(val.message) : String(val);
          post(msg);
        };
        try {
          Promise.resolve((0, eval)(String(m.code))).then(function (v) { reply(true, v); }, function (e) { reply(false, e); });
        } catch (e) { reply(false, e); }
        break;
      }
      case "rpc_result": {
        var p = pending.get(m.id);
        if (!p) return;
        pending.delete(m.id);
        if (m.ok) p.resolve(m.result); else p.reject(new Error(typeof m.error === "string" ? m.error : JSON.stringify(m.error)));
        break;
      }
      case "event": {
        if (m.name === "status") {
          var st = statusOf(m.payload), rid = runIdOf(m.payload);
          if (rid && st && TERMINAL[st]) settleRun(rid, m.payload);
        }
        emit(m.name, m.payload);
        break;
      }
    }
  });

  function run(inputs) {
    return rpc("run", { inputs: inputs || {} }).then(function (res) {
      var runId = runIdOf(res) || res;
      if (typeof runId !== "string") return res;
      if (res && TERMINAL[statusOf(res)]) return res;
      if (finishedRuns.has(runId)) { var r = finishedRuns.get(runId); finishedRuns.delete(runId); return r; }
      return new Promise(function (resolve, reject) { runWaiters.set(runId, { resolve: resolve, reject: reject }); });
    });
  }

  function assetUrl(path) {
    var enc = encodeURIComponent(path);
    var plat = info.platform || (/linux/i.test(navigator.platform || "") ? "linux" : "");
    return plat === "linux" ? "http://homemade-asset.localhost/" + enc : "homemade-asset://localhost/" + enc;
  }

  var api = {
    __bridge: true,
    ready: ready,
    get appId() { return info.appId; },
    get schema() { return info.schema; },
    get platform() { return info.platform; },
    run: run,
    call: function (method, params) { return rpc("call", { method: method, params: params || {} }); },
    stop: function () { return rpc("stop", {}); },
    pickFile: function (opts) { return rpc("pick_file", opts || { multiple: false, extensions: [] }); },
    pickFolder: function () { return rpc("pick_folder", {}); },
    assetUrl: assetUrl,
    getSchema: function () { return rpc("get_schema", {}); },
    getPresets: function () { return rpc("get_presets", {}); },
    savePreset: function (name, inputs) { return rpc("save_preset", { name: name, inputs: inputs || {} }); },
    listArtifacts: function () { return rpc("list_artifacts", {}); },
    reveal: function (path) { return rpc("reveal", { path: path }); },
    log: function () { var a = Array.prototype.slice.call(arguments); return rpc("log", { message: a.map(String).join(" ") }); },
    on: on,
    onStdout: function (cb) { return on("stdout", cb); },
    onStderr: function (cb) { return on("stderr", cb); },
    onProgress: function (cb) { return on("progress", cb); },
    onMetric: function (cb) { return on("metric", cb); },
    onArtifact: function (cb) { return on("artifact", cb); },
    onStatus: function (cb) { return on("status", cb); },
    onFileDrop: function (cb) { return on("file_drop", cb); },
    onWorkerReady: function (cb) { return on("worker_ready", cb); },
  };
  window.homemade = api;

  // Let OS file drops reach the host webview (which forwards them as `file_drop` events).
  function allowDrop(e) { e.preventDefault(); if (e.dataTransfer) e.dataTransfer.dropEffect = "copy"; }
  window.addEventListener("dragover", allowDrop);
  window.addEventListener("dragenter", allowDrop);
  window.addEventListener("drop", function (e) { e.preventDefault(); });

  function announce() { post({ type: "ready" }); }
  announce();
  // Re-announce until init arrives (host listener may attach after our first message).
  var tries = 0;
  var timer = setInterval(function () {
    if (info.appId !== null || ++tries > 20) { clearInterval(timer); return; }
    announce();
  }, 250);
})();
