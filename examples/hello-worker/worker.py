#!/usr/bin/env python3
"""Warm RPC worker example: exposes add(a, b) and slow(n)."""
import os
import sys
import time

_here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("HOMEMADE_RESOURCES", os.path.join(_here, "..", "..", "src-tauri", "resources")))

from homemade_worker import artifact, expose, metric, progress, run_worker  # noqa: E402


@expose
def add(a, b):
    return a + b


@expose
def slow(n=5):
    n = int(n)
    start = time.time()
    for i in range(n):
        time.sleep(0.5)
        progress(int((i + 1) * 100 / max(n, 1)), f"Step {i + 1}/{n}")
    os.makedirs(os.path.join(_here, "outputs"), exist_ok=True)
    out = os.path.join(_here, "outputs", "slow.txt")
    with open(out, "w") as f:
        f.write(f"slow({n}) finished in {time.time() - start:.2f}s\n")
    artifact(out)
    metric({"seconds": round(time.time() - start, 2)})
    return {"steps": n, "file": out}


if __name__ == "__main__":
    run_worker()
