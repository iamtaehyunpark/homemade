#!/bin/zsh
# Pre-step: plant an "orphan" — a detached process group holding a port, plus a runs row still marked running —
# exactly what a forced quit of a previous Homemade session leaves behind. The suite then asserts startup reaped it.
set -u
python3 - <<'PY'
import os, subprocess, sqlite3, time, socket, uuid
s=socket.socket(); s.bind(("127.0.0.1",0)); port=s.getsockname()[1]; s.close()
p=subprocess.Popen(["python3","-c",f"import http.server,socketserver;socketserver.TCPServer.allow_reuse_address=True\nwith socketserver.TCPServer(('127.0.0.1',{port}),http.server.SimpleHTTPRequestHandler) as s: s.serve_forever()"],
                   start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(1)
db=sqlite3.connect(os.path.expanduser("~/.homemade/state.db"))
for col in ("pgid INTEGER","port INTEGER"):
    try: db.execute("ALTER TABLE runs ADD COLUMN "+col)
    except sqlite3.OperationalError: pass
db.execute("DELETE FROM runs WHERE id LIKE 'orphan-e2e%'")
db.execute("INSERT INTO runs (id,app_id,status,inputs_json,cli_command,started_at,pgid,port) VALUES (?,?,?,?,?,?,?,?)",
           ("orphan-e2e","hello-cli","running","{}","python3 orphan", int(time.time()*1000), p.pid, port))
db.commit(); db.close()
open(os.environ["E"]+"/orphan.json","w").write('{"pid":%d,"port":%d}'%(p.pid,port))
print("planted orphan pid",p.pid,"port",port)
PY
