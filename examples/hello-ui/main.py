#!/usr/bin/env python3
"""Hello CLI: greets someone N times, reports progress, writes outputs/greeting.txt."""
import argparse
import os
import time


def main():
    p = argparse.ArgumentParser(description="Write a greeting file.")
    p.add_argument("input_file", nargs="?", default=None, help="Optional text file whose first line is appended")
    p.add_argument("--name", default="World", help="Who to greet")
    p.add_argument("--count", type=int, default=3, help="How many greetings")
    p.add_argument("--shout", action="store_true", help="Uppercase the output")
    a = p.parse_args()

    extra = ""
    if a.input_file:
        with open(a.input_file, encoding="utf-8", errors="replace") as f:
            extra = f.readline().strip()

    lines = []
    for i in range(a.count):
        line = f"Hello, {a.name}! ({i + 1}/{a.count})"
        if extra:
            line += f" -- {extra}"
        if a.shout:
            line = line.upper()
        print(line, flush=True)
        lines.append(line)
        pct = int((i + 1) * 100 / max(a.count, 1))
        print(f'@homemade:progress {pct} "Greeting {i + 1} of {a.count}"', flush=True)
        time.sleep(0.4)

    os.makedirs("outputs", exist_ok=True)
    out = os.path.abspath(os.path.join("outputs", "greeting.txt"))
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(f'@homemade:artifact "{out}"', flush=True)
    print(f"Wrote {out}", flush=True)


if __name__ == "__main__":
    main()
