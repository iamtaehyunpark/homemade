#!/bin/sh
# Build the Apple Foundation Model helper into src-tauri/resources/bin/homemade-fm.
# Never fails the surrounding build: skips (exit 0) on non-macOS, without swiftc, or when the SDK lacks FoundationModels.
DIR="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$DIR/swift/homemade-fm.swift"
OUT="$DIR/resources/bin/homemade-fm"
if [ "$(uname -s)" != "Darwin" ]; then echo "[build-fm] not macOS; skipping local model helper"; exit 0; fi
if ! command -v swiftc >/dev/null 2>&1; then echo "[build-fm] swiftc not found; skipping local model helper"; exit 0; fi
mkdir -p "$DIR/resources/bin"
if [ -x "$OUT" ] && [ "$OUT" -nt "$SRC" ]; then echo "[build-fm] up to date"; exit 0; fi
if swiftc -O -parse-as-library -framework FoundationModels -o "$OUT" "$SRC"; then
  chmod +x "$OUT"; echo "[build-fm] built $OUT"
else
  rm -f "$OUT"; echo "[build-fm] swiftc failed (needs macOS 26 SDK); continuing without local model helper"
fi
exit 0
