#!/bin/zsh
# Builds debug binary, runs the live E2E, reacts to TOUCH_NOW by touching hello-ui files.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
E=/private/tmp/claude-501/-Users-t-github-guix/acfc7ab3-3b6e-46ea-9564-96c0b1947ef3/scratchpad/e2e
cd /Users/t/github/homemade
pkill -f target/debug/homemade; pkill -f "vite" ; sleep 1
# Fixture prep: venv-app must be an unmanaged venv over 50 MB (badge threshold) on every run; orphan.sh plants the orphan + test home.
rm -rf $E/venv-app/.venv; python3 -m venv --copies $E/venv-app/.venv >/dev/null 2>&1 && $E/venv-app/.venv/bin/pip install -q six >/dev/null 2>&1
mkdir -p $E/venv-app/.venv/lib/pad && dd if=/dev/zero of=$E/venv-app/.venv/lib/pad/pad.bin bs=1m count=60 2>/dev/null
rm -rf $E/home; E=$E zsh tests/e2e/orphan.sh
LOG=$E/e2e.log; : > $LOG
HOMEMADE_HOME=$E/home HOMEMADE_E2E=$E/run.js pnpm tauri dev 2>&1 | grep --line-buffered -vE "^\s*(Compiling|Building|Checking|Downloaded|Blocking|Running|Finished|Info|\$|➜|VITE)" | tee $LOG | while IFS= read -r line; do
  case "$line" in
    *"TOUCH_NOW"*) (sleep 1; touch $E/hello-ui/ui/index.html; sleep 1; printf '\n# touched\n' >> $E/hello-ui/homemade.toml) & ;;
  esac
  echo "$line"
done
echo "EXIT=${pipestatus[1]}"
