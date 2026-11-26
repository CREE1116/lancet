#!/usr/bin/env python3
"""Large-corpus stress benchmark: index build time, peak RSS, incremental sync,
compaction, and query latency (lancet vs rg / fd / ast-grep) on a big checkout
such as torvalds/linux (~90k files).

Usage: python3 bench/scale_bench.py --corpus bench/corpora/linux
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LANCET = ROOT / "target" / "release" / "lancet"
TOOLS = ROOT / "bench" / ".cache" / "tools"


def tool(name: str) -> str | None:
    p = TOOLS / name
    if p.exists() and os.access(p, os.X_OK):
        return str(p)
    return shutil.which(name)


def timed(cmd: list[str], runs: int = 5, warmup: int = 1) -> tuple[float, subprocess.CompletedProcess]:
    for _ in range(warmup):
        subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    ts, last = [], None
    for _ in range(runs):
        t0 = time.perf_counter()
        last = subprocess.run(cmd, capture_output=True, text=True)
        ts.append((time.perf_counter() - t0) * 1000.0)
    return statistics.median(ts), last


def run_rss(cmd: list[str]) -> tuple[float, float, subprocess.CompletedProcess]:
    """Return (wall_ms, peak_rss_mib, proc) using macOS `/usr/bin/time -l`."""
    t0 = time.perf_counter()
    p = subprocess.run(["/usr/bin/time", "-l", *cmd], capture_output=True, text=True)
    wall = (time.perf_counter() - t0) * 1000.0
    m = re.search(r"(\d+)\s+maximum resident set size", p.stderr)
    rss = int(m.group(1)) / (1024 * 1024) if m else float("nan")
    return wall, rss, p


def du_mib(path: Path) -> float:
    total = 0
    for dp, _, fns in os.walk(path):
        for fn in fns:
            try:
                total += (Path(dp) / fn).stat().st_size
            except OSError:
                pass
    return total / (1024 * 1024)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", type=Path, required=True)
    ap.add_argument("--index-dir", type=Path, default=ROOT / "bench" / ".cache" / "scale-idx")
    args = ap.parse_args()
    corpus = args.corpus.resolve()
    idx = args.index_dir.resolve()
    rg, fd, ag = tool("rg"), tool("fd"), tool("ast-grep")

    if idx.exists():
        shutil.rmtree(idx)
    nfiles_cmd = [rg or "rg", "--files", "--no-ignore", "--hidden", "-g", "!.git", str(corpus)]
    nfiles = len(subprocess.run(nfiles_cmd, capture_output=True, text=True).stdout.splitlines())
    corpus_mib = du_mib(corpus) - du_mib(corpus / ".git")
    print(f"Corpus: {corpus.name} | files={nfiles} | size={corpus_mib:.0f} MiB (excl .git)")

    wall, rss, p = run_rss([str(LANCET), "index", str(corpus), "--index-dir", str(idx), "--no-ignore"])
    rep = json.loads(p.stdout)
    idx_mib = du_mib(idx)
    print(
        f"Cold index build : {wall/1000:.2f} s | peak RSS {rss:.0f} MiB | segments={rep['segments']} "
        f"| index={idx_mib:.0f} MiB ({idx_mib/corpus_mib*100:.1f}% of corpus)"
    )

    wall, rss, p = run_rss([str(LANCET), "index", str(corpus), "--index-dir", str(idx), "--no-ignore"])
    print(f"No-op re-sync    : {wall:.0f} ms | peak RSS {rss:.0f} MiB")

    # Mutate 200 files then incremental sync, then compact.
    srcs = sorted(Path(l) for l in subprocess.run(
        [rg or "rg", "--files", "--no-ignore", "-g", "*.c", str(corpus)], capture_output=True, text=True
    ).stdout.splitlines())[:200]
    originals = {}
    for f in srcs:
        originals[f] = f.read_bytes()
        f.write_bytes(originals[f] + b"\n/* lancet_scale_probe_token */\n")
    try:
        wall, rss, p = run_rss([str(LANCET), "index", str(corpus), "--index-dir", str(idx), "--no-ignore"])
        rep = json.loads(p.stdout)
        print(f"Incremental sync : {wall:.0f} ms (+{rep['added']} added) | peak RSS {rss:.0f} MiB | segments={rep['segments']}")
        wall, rss, p = run_rss([str(LANCET), "index", str(corpus), "--index-dir", str(idx), "--no-ignore", "--compact"])
        rep = json.loads(p.stdout)
        print(f"Compaction       : {wall:.0f} ms | segments={rep['segments']} | compacted={rep.get('compacted', 0)}")

        q = '"lancet_scale_probe_token" | count'
        _, auto = timed([str(LANCET), q, str(corpus), "--index-dir", str(idx), "--no-ignore", "--json"], 3)
        print(f"Freshness check  : auto sees {json.loads(auto.stdout)['matched']} probe hits (expected 200)")
    finally:
        for f, b in originals.items():
            f.write_bytes(b)
    subprocess.run([str(LANCET), "index", str(corpus), "--index-dir", str(idx), "--no-ignore"], capture_output=True)

    print("\nQuery latency (median of 5, ms):")
    hdr = f"{'query':<34} | {'rg':>8} | {'lancet scan':>11} | {'lancet auto':>11} | {'lancet trust':>12}"
    print(hdr)
    print("-" * len(hdr))
    cases = [
        ("rare literal (PM_RESUME)", "PM_RESUME"),
        ("rare ident (kmalloc_node_track)", "kmalloc_node_track_caller"),
        ("common word (spin_lock)", "spin_lock"),
        ("zero hit", "zzz_definitely_not_in_linux_zzz"),
    ]
    for label, lit in cases:
        rg_ms = timed([rg, "--no-ignore", "-i", "-l", "-F", lit, str(corpus)], 3)[0] if rg else float("nan")
        row = [timed([str(LANCET), f'"{lit}" | count', str(corpus), "--no-ignore", "--json", *fl,
                      *( ["--index-dir", str(idx)] if "--no-index" not in fl else [])], 3)[0]
               for fl in (["--no-index"], [], ["--trust-index"])]
        print(f"{label:<34} | {rg_ms:8.1f} | {row[0]:11.1f} | {row[1]:11.1f} | {row[2]:12.1f}")

    if fd:
        print("\nFile discovery:")
        a = timed([fd, "--no-ignore", "-t", "f", "-e", "c", ".", str(corpus)], 3)[0]
        b = timed([str(LANCET), "fd", "--no-ignore", "-t", "f", "-e", "c", str(corpus), "--json"], 3)[0]
        c = timed([str(LANCET), "fd", "--trust-index", "--index-dir", str(idx), "-t", "f", "-e", "c", str(corpus), "--json"], 3)[0]
        print(f"  *.c files          fd={a:.1f} ms | lancet fd={b:.1f} ms | lancet fd --trust-index={c:.1f} ms")

    if ag:
        print("\nAST search (C):")
        pat = "kmalloc($SZ, $FLAGS)"
        a = timed([ag, "run", "-p", pat, "-l", "c", "--no-ignore", "hidden", "--no-ignore", "vcs", str(corpus), "--json=compact"], 1, 0)[0]
        b = timed([str(LANCET), "ast", pat, str(corpus), "--lang", "c", "--no-ignore", "--no-index", "--json"], 1, 0)[0]
        c = timed([str(LANCET), "ast", pat, str(corpus), "--lang", "c", "--no-ignore", "--index-dir", str(idx), "--json"], 3)[0]
        d = timed([str(LANCET), "ast", pat, str(corpus), "--lang", "c", "--no-ignore", "--trust-index", "--index-dir", str(idx), "--json"], 3)[0]
        print(f"  {pat}: ast-grep={a:.0f} ms | lancet scan={b:.0f} | auto={c:.0f} | trust={d:.0f}")


if __name__ == "__main__":
    main()
