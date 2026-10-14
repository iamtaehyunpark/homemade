#!/usr/bin/env python3
"""A bare argparse script with no homemade.toml, used to exercise manifest inference."""
import argparse
import os


def main():
    parser = argparse.ArgumentParser(description="List files in a folder matching a pattern")
    parser.add_argument("folder", help="Folder to scan")
    parser.add_argument("--pattern", type=str, default="*.png", help="Glob pattern")
    parser.add_argument("--limit", type=int, default=10, help="Max files to list")
    parser.add_argument("--scale", type=float, default=1.0, help="Fake scale factor")
    parser.add_argument("--mode", choices=["fast", "accurate"], default="fast")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    import glob
    files = sorted(glob.glob(os.path.join(args.folder, args.pattern)))[: args.limit]
    for i, f in enumerate(files):
        print(f"{i + 1}. {f} (scale={args.scale}, mode={args.mode})")
    if args.verbose:
        print(f"{len(files)} file(s) matched")


if __name__ == "__main__":
    main()
