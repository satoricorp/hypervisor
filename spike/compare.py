#!/usr/bin/env python3
"""compare.py — oracle check for M1: does hvscan (Rust) match hvwatch (Python)?

Usage: python3 spike/compare.py [--max-age 48] [--limit 8]

Runs both scanners with identical flags, joins sessions on (harness, sid), and
diffs the stable fields. `state`/`age`/`mtime` are compared leniently (both
scanners run seconds apart; a session may legitimately flip working<->done).
Exit 0 and prints OK when everything matches.
"""
import argparse
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STRICT = ["title", "model", "cwd", "branch", "repo", "sidechains"]
LENIENT = ["state", "last_user", "last_assistant", "activity"]

def run(cmd):
    out = subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT)
    if out.returncode != 0:
        sys.exit(f"FAIL: {' '.join(cmd)} exited {out.returncode}\n{out.stderr[-2000:]}")
    rows = {}
    for line in out.stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            s = json.loads(line)
            rows[(s["harness"], s["sid"])] = s
        except (ValueError, KeyError):
            sys.exit(f"FAIL: bad JSON line from {' '.join(cmd)}:\n{line[:300]}")
    return rows

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--max-age", default="48")
    ap.add_argument("--limit", default="8")
    a = ap.parse_args()
    flags = ["--json", "--max-age", a.max_age, "--limit", a.limit]

    py = run(["python3", "spike/hvwatch.py", *flags])
    rs = run(["cargo", "run", "--quiet",
              "--manifest-path", "src-tauri/Cargo.toml", "--bin", "hvscan", "--", *flags])

    problems, warned = [], 0
    only_py = set(py) - set(rs)
    only_rs = set(rs) - set(py)
    # one-session drift is tolerable (a session may age out between the two runs)
    if len(only_py) > 1 or len(only_rs) > 1:
        problems.append(f"session sets differ — only in python: {sorted(only_py)}; "
                        f"only in rust: {sorted(only_rs)}")

    for key in sorted(set(py) & set(rs)):
        p, r = py[key], rs[key]
        for f in STRICT:
            if p.get(f) != r.get(f):
                problems.append(f"{key} field {f!r}: python={p.get(f)!r} rust={r.get(f)!r}")
        for f in LENIENT:
            if p.get(f) != r.get(f):
                warned += 1
                print(f"  lenient diff {key} {f!r}: python={str(p.get(f))[:60]!r} "
                      f"rust={str(r.get(f))[:60]!r}", file=sys.stderr)

    print(f"compared {len(set(py) & set(rs))} sessions "
          f"({len(py)} python / {len(rs)} rust) · {warned} lenient diffs")
    if problems:
        print("MISMATCH:")
        for p in problems[:30]:
            print(f"  - {p}")
        sys.exit(1)
    print("OK")

if __name__ == "__main__":
    main()
