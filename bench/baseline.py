#!/usr/bin/env python3
"""Per-command timings on the pinned corpora, to compare restructuring steps.

Each command runs without the index (the engine itself) on a fixed corpus; the
median of 5 runs after one warm-up is reported. Run it before and after a change
and compare the two tables (docs/ARCHITECTURE.md, section 3.3).

  python3 bench/baseline.py                  # print a Markdown table
  python3 bench/baseline.py --save NAME      # also write bench/results/baseline-NAME.md
"""

from __future__ import annotations

import argparse
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPORA = ROOT / "bench" / "corpora"
LANCET = str(ROOT / "target" / "release" / "lancet")

# (what it shows, corpus subdirectory, lancet arguments)
COMMANDS = [
    ("rg literal", "rust", ["rg", "-c", "ThreadPoolBuilder", "--no-index"]),
    ("rg regex", "linux", ["rg", "-c", r"spin_lock_irq\w*\(", "--no-index"]),
    ("refs, Rust (4 parses/file today)", "rust/compiler", ["refs", "span_bug", "--no-index"]),
    ("refs, Python", "django", ["refs", "get_object_or_404", "--no-index"]),
    ("refs, Go", "kubernetes/pkg", ["refs", "NewController", "--no-index"]),
    ("refs --rank", "django/django/db", ["refs", "--rank", "*Field", "--kind", "class", "--no-index"]),
    ("outline, a directory", "django/django/db/models", ["outline"]),
    ("ast, literal pattern", "rust/compiler/rustc_middle", ["ast", "$X.unwrap()", "--lang", "rs", "--no-index"]),
    ("ast, no literal", "django/django/db", ["ast", "if $C: return $X", "--lang", "py", "--no-index"]),
    ("read --symbol", "rust/compiler/rustc_middle/src/ty/context.rs", ["read", "--symbol", "TyCtxt"]),
]


def median_ms(cmd: list[str], runs: int = 5) -> tuple[float, int]:
    subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    times = []
    out = b""
    for _ in range(runs):
        t = time.perf_counter()
        p = subprocess.run(cmd, capture_output=True)
        times.append((time.perf_counter() - t) * 1000)
        out = p.stdout
    return statistics.median(times), out.count(b"\n")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--save", metavar="NAME")
    args = ap.parse_args()
    rev = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT,
                         capture_output=True, text=True).stdout.strip()
    rows = [f"Commit `{rev}`, median of 5 runs after a warm-up, no index.\n",
            "| Command | Corpus | ms | output lines |", "|---|---|---:|---:|"]
    for label, sub, cmd in COMMANDS:
        target = CORPORA / sub
        if not target.exists():
            rows.append(f"| {label} | `{sub}` | (missing) | |")
            continue
        # The path goes where each command expects it: last for most, first for read.
        full = [LANCET, *cmd[:1], str(target), *cmd[1:]] if cmd[0] == "read" else [LANCET, *cmd, str(target)]
        ms, lines = median_ms(full)
        rows.append(f"| {label} | `{sub}` | {ms:.1f} | {lines} |")
        print(rows[-1], flush=True)
    text = "\n".join(rows) + "\n"
    if args.save:
        out = ROOT / "bench" / "results" / f"baseline-{args.save}.md"
        out.write_text(f"# Baseline: {args.save}\n\n{text}")
        print(f"wrote {out}")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
