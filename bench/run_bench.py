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

    # (label, ripgrep arguments): lancet rg runs the same arguments in each index mode
    # and must print the same files as ripgrep.
    benchmarks = [
        ("zero_hit (long miss)", ["--no-ignore", "-i", "-l", "-F", "definitely_not_in_any_crate_xyz"]),
        ("short_2b (fn)", ["--no-ignore", "-i", "-l", "-F", "fn"]),
        ("ext_filter (edition:toml)", ["--no-ignore", "-i", "-l", "-F", "-g", "*.toml", "edition"]),
        ("common_word (function)", ["--no-ignore", "-i", "-l", "-F", "function"]),
        ("rare_lit (tokio_unstable)", ["--no-ignore", "-i", "-l", "-F", "tokio_unstable"]),
        ("med_lit (ThreadPoolBuilder)", ["--no-ignore", "-i", "-l", "-F", "ThreadPoolBuilder"]),
        ("common_lit (unwrap_or_else)", ["--no-ignore", "-i", "-l", "-F", "unwrap_or_else"]),
        ("regex_anchored", ["--no-ignore", "-l", r"pub unsafe fn \w+\("]),
    ]

    header = f"{'Query':<27} | {'rg ms (hits)':>14} | {'no-index ms':>12} | {'auto ms':>10} | {'trust ms':>10}"
    print(header)
    print("-" * len(header))

    for label, args in benchmarks:
        rg_med, _, p_rg = run_timed(["rg", *args, str(corpus)])
        want = sorted(ln for ln in p_rg.stdout.splitlines() if ln)
        cols = []
        for mode_flags in (["--no-index"], [], ["--trust-index"]):
            cmd = [str(WEFT), "rg", *args, "--index-dir", str(idx_dir), *mode_flags, str(corpus)]
            med, _, proc = run_timed(cmd)
            got = sorted(ln for ln in proc.stdout.splitlines() if ln)
            cols.append(f"{med:6.1f}" + ("" if got == want else " ≠rg"))
        print(f"{label:<27} | {rg_med:6.1f} ({len(want)}){'':>2} | {cols[0]:>12} | {cols[1]:>10} | {cols[2]:>10}")

if __name__ == "__main__":
    main()
