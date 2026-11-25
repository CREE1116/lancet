#!/usr/bin/env python3
"""Benchmark lancet (scan, auto trigram index, trusted trigram index) against ripgrep."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import statistics
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WEFT = ROOT / "target" / "release" / "lancet"


def dir_size_bytes(path: Path) -> int:
    total = 0
    for dp, _, fns in os.walk(path):
        for fn in fns:
            try:
                total += (Path(dp) / fn).stat().st_size
            except OSError:
                pass
    return total


def run_timed(cmd: list[str], warmup: int = 1, runs: int = 5) -> tuple[float, float, subprocess.CompletedProcess]:
    for _ in range(warmup):
        subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    times = []
    last = None
    for _ in range(runs):
        t0 = time.perf_counter()
        last = subprocess.run(cmd, capture_output=True, text=True)
        times.append((time.perf_counter() - t0) * 1000.0)
    assert last is not None
    return statistics.median(times), min(times), last


def rg_and_count_files(pat1: str, pat2: str, corpus: Path) -> tuple[float, float, int]:
    def once() -> int:
        p1 = subprocess.run(
            ["rg", "--no-ignore", "-i", "-l", "-F", pat1, str(corpus)],
            capture_output=True,
            text=True,
        )
        files = [ln for ln in p1.stdout.splitlines() if ln]
        if not files:
            return 0
        cnt = 0
        for i in range(0, len(files), 500):
            chunk = files[i : i + 500]
            q = subprocess.run(["rg", "--no-ignore", "-i", "-l", "-F", pat2, *chunk], capture_output=True, text=True)
            cnt += len([ln for ln in q.stdout.splitlines() if ln])
        return cnt

    once()
    times = []
    cnt = 0
    for _ in range(5):
        t0 = time.perf_counter()
        cnt = once()
        times.append((time.perf_counter() - t0) * 1000.0)
    return statistics.median(times), min(times), cnt


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", type=Path, default=Path.home() / ".cargo" / "registry" / "src")
    ap.add_argument("--index-dir", type=Path, default=ROOT / "bench" / ".cache" / "cargo-idx")
    args = ap.parse_args()

    corpus: Path = args.corpus.expanduser().resolve()
    idx_dir: Path = args.index_dir.resolve()
    if not WEFT.exists():
        raise SystemExit(f"Release binary missing: {WEFT} (run cargo build --release)")
    if not corpus.exists():
        raise SystemExit(f"Corpus missing: {corpus}")

    if idx_dir.exists():
        shutil.rmtree(idx_dir)
    idx_dir.mkdir(parents=True, exist_ok=True)

    corpus_mb = dir_size_bytes(corpus) / (1024 * 1024)
    bin_mb = WEFT.stat().st_size / (1024 * 1024)
    print(f"Binary: {WEFT} ({bin_mb:.2f} MiB) | Corpus: {corpus} ({corpus_mb:.1f} MiB)")

    t0 = time.perf_counter()
    p_idx = subprocess.run(
        [str(WEFT), "index", str(corpus), "--index-dir", str(idx_dir), "--no-ignore"],
        capture_output=True,
        text=True,
        check=True,
    )
    cold_ms = (time.perf_counter() - t0) * 1000.0
    rep = json.loads(p_idx.stdout)
    idx_mb = dir_size_bytes(idx_dir) / (1024 * 1024)

    t0 = time.perf_counter()
    subprocess.run(
        [str(WEFT), "index", str(corpus), "--index-dir", str(idx_dir), "--no-ignore"],
        capture_output=True,
        text=True,
        check=True,
    )
    noop_ms = (time.perf_counter() - t0) * 1000.0
    print(
        f"Index build: {cold_ms:.1f} ms | files={rep['files']} | segs={rep['segments']} "
        f"| size={idx_mb:.1f} MiB ({idx_mb / corpus_mb * 100:.1f}%) | noop_sync={noop_ms:.1f} ms\n"
    )

    benchmarks = [
        (
            "zero_hit (long miss)",
            'find "definitely_not_in_any_crate_xyz" | count',
            ["rg", "--no-ignore", "-i", "-l", "-F", "definitely_not_in_any_crate_xyz", str(corpus)],
        ),
        (
            "short_2b (fn)",
            'find "fn" | count',
            ["rg", "--no-ignore", "-i", "-l", "-F", "fn", str(corpus)],
        ),
        (
            "ext_filter (edition:toml)",
            'find "edition" in ext:toml | count',
            ["rg", "--no-ignore", "-i", "-l", "-F", "-g", "*.toml", "edition", str(corpus)],
        ),
        (
            "common_word (function)",
            'find "function" | count',
            ["rg", "--no-ignore", "-i", "-l", "-F", "function", str(corpus)],
        ),
        (
            "rare_lit (tokio_unstable)",
            'find "tokio_unstable" | count',
            ["rg", "--no-ignore", "-i", "-l", "-F", "tokio_unstable", str(corpus)],
        ),
        (
            "med_lit (ThreadPoolBuilder)",
            'find "ThreadPoolBuilder" | count',
            ["rg", "--no-ignore", "-i", "-l", "-F", "ThreadPoolBuilder", str(corpus)],
        ),
        (
            "common_lit (unwrap_or_else)",
            'find "unwrap_or_else" | count',
            ["rg", "--no-ignore", "-i", "-l", "-F", "unwrap_or_else", str(corpus)],
        ),
        (
            "boolean_and (unsafe&trans)",
            'find "unsafe" AND "transmute" NOT "miri" | count',
            ("AND", "unsafe", "transmute"),
        ),
        (
            "regex_anchored",
            'find regex("pub unsafe fn \\w+\\(") | count',
            ["rg", "--no-ignore", "-l", r"pub unsafe fn \w+\(", str(corpus)],
        ),
        (
            "near_15",
            'find near("unsafe", "transmute", 15) | count',
            None,
        ),
        (
            "fuzzy_1 (transmte)",
            'find fuzzy("transmte", 1) | count',
            None,
        ),
        (
            "in_function",
            'find "transmute" in function | count',
            None,
        ),
        (
            "symbol (ThreadPoolBuilder)",
            'find symbol("ThreadPoolBuilder") | count',
            None,
        ),
        (
            "rare_lit (limit 5)",
            'find "tokio_unstable" | limit 5',
            None,
        ),
    ]

    header = f"{'Query':<27} | {'rg ms (hits)':>14} | {'scan ms':>8} | {'auto ms (cands)':>17} | {'trust ms (hits)':>17}"
    print(header)
    print("-" * len(header))

    for label, wq, rg_spec in benchmarks:
        if isinstance(rg_spec, list):
            rg_med, _, p_rg = run_timed(rg_spec)
            rg_hits = len([ln for ln in p_rg.stdout.splitlines() if ln])
            rg_col = f"{rg_med:5.1f} ({rg_hits})"
        elif isinstance(rg_spec, tuple) and rg_spec[0] == "AND":
            rg_med, _, rg_hits = rg_and_count_files(rg_spec[1], rg_spec[2], corpus)
            rg_col = f"{rg_med:5.1f} ({rg_hits})"
        else:
            rg_col = "-"

        cols = []
        for mode_flags in (["--no-index"], [], ["--trust-index"]):
            cmd = [str(WEFT), wq, str(corpus), "--index-dir", str(idx_dir), "--no-ignore", "--json", *mode_flags]
            med, _, proc = run_timed(cmd)
            data = json.loads(proc.stdout)
            if not mode_flags:
                cands = data["stats"].get("index_candidates", "-")
                cols.append(f"{med:6.1f} ({cands})")
            else:
                cols.append(f"{med:6.1f} ({data['matched']})")

        print(f"{label:<27} | {rg_col:>14} | {cols[0]:>16} | {cols[1]:>17} | {cols[2]:>17}")

    batch_queries = [
        'find "tokio_unstable" | count',
        'find "ThreadPoolBuilder" | count',
        'find "definitely_not_in_any_crate_xyz" | count',
        'find "unsafe" AND "transmute" NOT "miri" | count',
        'find symbol("ThreadPoolBuilder") | count',
        'find regex("tokio_unstable|ThreadPoolBuilder") | count',
    ]
    bflags = [a for q in batch_queries for a in ("-q", q)]
    b_cols = []
    for mode_flags in (["--no-index"], [], ["--trust-index"]):
        cmd = [str(WEFT), "batch", str(corpus), "--index-dir", str(idx_dir), "--no-ignore", *mode_flags, *bflags]
        med, _, proc = run_timed(cmd, runs=3)
        b_data = json.loads(proc.stdout)
        b_cols.append(f"{med:6.1f} ({len(b_data['results'])}q)")
    print(f"{'batch_6q':<27} | {'-':>14} | {b_cols[0]:>16} | {b_cols[1]:>17} | {b_cols[2]:>17}")


if __name__ == "__main__":
    main()
