#!/bin/zsh
# Pre-step (suite setup): test-home config + fake agent session, and plant an "orphan" — a detached process group holding a port, plus a runs row still marked running —
# exactly what a forced quit of a previous Homemade session leaves behind. The suite then asserts startup reaped it.
set -u
python3 - <<'PY'
import os, subprocess, sqlite3, time, socket, uuid
s=socket.socket(); s.bind(("127.0.0.1",0)); port=s.getsockname()[1]; s.close()
p=subprocess.Popen(["python3","-c",f"import http.server,socketserver;socketserver.TCPServer.allow_reuse_address=True\nwith socketserver.TCPServer(('127.0.0.1',{port}),http.server.SimpleHTTPRequestHandler) as s: s.serve_forever()"],
                   start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(1)
home=os.path.join(os.environ["E"], "home"); os.makedirs(home, exist_ok=True)
dbp=os.path.join(home, "state.db")
if not os.path.exists(dbp):  # first run against a fresh test home: clone the schema from the real DB
    schema=subprocess.run(["sqlite3", os.path.expanduser("~/.homemade/state.db"), ".schema"], capture_output=True, text=True).stdout
    subprocess.run(["sqlite3", dbp], input=schema, text=True)
db=sqlite3.connect(dbp)
# Test-home config: alias used by env-app ($E2E_ALIAS/sub) — secrets names are managed by the app itself.
cfg=os.path.join(home, "config.toml")
if not os.path.exists(cfg) or "E2E_ALIAS" not in open(cfg).read():
    open(cfg, "w").write('[aliases]\nE2E_ALIAS = "/tmp/e2e-alias"\n\n[secrets]\nnames = []\n')
# Fake Claude Code session for the hello-cli fixture: ~/.claude/projects/<root with / -> ->/<uuid>.jsonl
root=os.path.join(os.environ["E"], "hello-cli")
proj=os.path.join(os.path.expanduser("~/.claude/projects"), root.replace("/", "-")); os.makedirs(proj, exist_ok=True)
import json as _j
with open(os.path.join(proj, "11111111-2222-3333-4444-555555555555.jsonl"), "w") as f:
    f.write(_j.dumps({"type":"user","sessionId":"11111111-2222-3333-4444-555555555555","cwd":root,"message":{"role":"user","content":"E2E fake session: please add a flag"}})+"\n")
    f.write(_j.dumps({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"ok"}]}})+"\n")
for col in ("pgid INTEGER","port INTEGER"):
    try: db.execute("ALTER TABLE runs ADD COLUMN "+col)
    except sqlite3.OperationalError: pass
db.execute("DELETE FROM runs WHERE id LIKE 'orphan-e2e%'")
db.execute("INSERT OR IGNORE INTO apps (id,name,root_dir,manifest_hash,execution_mode,created_at) VALUES ('orphan-host','Orphan Host','/tmp/orphan-host','','cli',0)")
db.execute("INSERT INTO runs (id,app_id,status,inputs_json,cli_command,started_at,pgid,port) VALUES (?,?,?,?,?,?,?,?)",
           ("orphan-e2e","orphan-host","running","{}","python3 orphan", int(time.time()*1000), p.pid, port))
db.commit(); db.close()
open(os.environ["E"]+"/orphan.json","w").write('{"pid":%d,"port":%d}'%(p.pid,port))
print("planted orphan pid",p.pid,"port",port)
PY
