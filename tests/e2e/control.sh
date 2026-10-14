#!/bin/zsh
# Driven by runner.sh when run.js prints CONTROL_NOW: exercises the CLI and the MCP server against the live app
# (HOMEMADE_HOME = test home). Writes PASS/FAIL lines + "DONE ok|fail" to $E/control.result, which run.js reads back.
set -u
B=${B:-src-tauri/target/debug/homemade}
export HOMEMADE_HOME=$E/home
OUT=$E/control.result; : > $OUT
fails=0
chk() { if eval "$2" >/dev/null 2>&1; then echo "PASS $1" >> $OUT; else echo "FAIL $1" >> $OUT; fails=$((fails+1)); fi }
chk "cli status"            '$B status | grep -q "running (pid"'
chk "cli apps lists hello-cli" '$B apps --json | grep -q "\"id\": \"hello-cli\""'
chk "cli app shows manifest" '$B app hello-cli | grep -q "\"manifest\""'
chk "cli cli builds command" '$B cli hello-cli name=Zed | grep -q "Zed"'
chk "cli unknown input rejected" '! $B cli hello-cli bogus=1 2>/dev/null'
run_out=$($B run hello-cli name=FromCLI count=1 --wait 2>/dev/null); rc=$?
chk "cli run --wait succeeds (rc=$rc out=$run_out)" '[ $rc -eq 0 ] && echo "$run_out" | grep -q "^success"'
chk "cli runs history"      '$B runs hello-cli | grep -q "success"'
chk "cli artifacts"         '$B artifacts hello-cli | grep -q "greeting.txt"'
chk "cli ports"             '$B ports >/dev/null'
chk "cli resources"         '$B resources | grep -q "cpuPct"'
chk "cli git status"        '$B git hello-cli status | grep -q "initialized"'
chk "cli presets save/list" '$B preset-save hello-cli CliPreset name=P >/dev/null && $B presets hello-cli | grep -q "CliPreset"'
chk "cli view launch"       '$B view launch'
chk "cli open hello-cli"    '$B open hello-cli'
# Monitoring + raw control against a live program: webapp-app serves for a while.
wrid=$($B run webapp-app 2>/dev/null | head -1)
sleep 3
chk "cli ps shows process tree"   '$B ps webapp-app | grep -q "PID"'
chk "cli top has current usage"   '$B top webapp-app --seconds 5 | grep -q "cpu"'
chk "cli activity running"        '$B activity webapp-app | grep -q "running"'
chk "cli log works on a live run"  '$B log $wrid >/dev/null && $B activity webapp-app --json | grep -q logTail'
chk "cli capture main window png" 'f=$($B capture) && [ -s "$f" ] && file "$f" | grep -q PNG'
chk "cli record 3s writes jsonl"  'f=$($B record webapp-app --seconds 3 2>/dev/null) && [ -s "$f" ] && grep -q "\"kind\":\"usage\"" "$f"'
chk "cli signal TERM stops run"   '$B signal $wrid TERM && $B wait $wrid --timeout 20 >/dev/null; $B runs webapp-app --limit 1 | grep -q "killed"'
# MCP over stdio: initialize, tools/list, tools/call status + start_run/wait_run
mcp_out=$(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"status","arguments":{}}}' \
  '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"start_run","arguments":{"app_id":"hello-cli","inputs":{"name":"FromMCP","count":1}}}}' \
  | $B --mcp 2>$E/mcp.err); printf "%s\n" "$mcp_out" > $E/mcp.out
chk "mcp initialize"        'printf "%s\n" "$mcp_out" | grep -q "protocolVersion"'
ntools=$(printf "%s\n" "$mcp_out" | grep '"id":2' | grep -o '"name":"[a-z_]*"' | wc -l | tr -d ' ')
chk "mcp tools/list has >= 30 tools ($ntools)" '[ "$ntools" -ge 30 ]'
chk "mcp status running"    'printf "%s\n" "$mcp_out" | grep "\"id\":3" | grep -qF "\\\"running\\\": true"'
rid=$(printf "%s\n" "$mcp_out" | grep '"id":4' | python3 -c 'import sys,json; r=json.loads(sys.stdin.readline()); print(json.loads(r["result"]["content"][0]["text"])["id"])' 2>/dev/null)
chk "mcp start_run returned id ($rid)" '[ -n "$rid" ]'
wait_out=$(printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"wait_run\",\"arguments\":{\"run_id\":\"$rid\"}}}" | $B --mcp 2>/dev/null)
chk "mcp wait_run success"  'printf "%s\n" "$wait_out" | grep -qF "\\\"status\\\": \\\"success\\\""'
echo "DONE $([ $fails -eq 0 ] && echo ok || echo fail)" >> $OUT
