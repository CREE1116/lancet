#!/usr/bin/env python3
"""Multi-language benchmark suite: lancet vs ripgrep, ugrep, git grep, zoekt, fd, ast-grep.

For every corpus (pinned tags, see CORPORA) it measures:
  * index: lancet cold build (time, peak RSS, size), no-op sync, zoekt-index build
  * text queries: rare literal, common identifier, -i, -w, regex with a literal,
    literal-free regex (index-proof worst case), no-hit, -c
  * AST: a pattern with a literal, and a literal-free structural pattern
  * file discovery: -e <ext>
Every lancet rg mode (auto / --trust-index / --no-index) is checked byte for
byte against ripgrep's output; AST match counts are compared with ast-grep.

Timing: hyperfine -N, warm cache, median of --runs after one warmup.
Results: bench/results/suite-<stamp>.json and bench/results/SUITE.md.

Usage: python3 bench/suite.py [--corpus NAME ...] [--runs 5] [--skip-index]
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORPORA_DIR = ROOT / "bench" / "corpora"
CACHE = ROOT / "bench" / ".cache"
TOOLS = CACHE / "tools"
RESULTS = ROOT / "bench" / "results"
LANCET = ROOT / "target" / "release" / "lancet"

# name -> pinned tag, lancet/ast-grep language, file extension, queries.
# Terms were chosen per corpus so "rare" hits a handful of files and "common"
# thousands of lines (counts are recorded in the results).
CORPORA: dict[str, dict] = {
    "linux": {
        "tag": "v6.12", "lang": "c", "ext": "c",
        "rare": "PM_RESUME", "common": "spin_lock", "icase": "mutex_lock",
        "regex": r"\w+_irqsave\(",
        "ast_lit": "kmalloc($SZ, $FLAGS)", "ast_nolit": "if ($C) { $$$B }",
    },
    "llvm": {
        "tag": "llvmorg-19.1.0", "lang": "cpp", "ext": "cpp",
        "rare": "getNumSuccessors", "common": "getContext", "icase": "dyn_cast",
        "regex": r"llvm::\w+Pass\b",
        "ast_lit": "dyn_cast<$T>($X)", "ast_nolit": "if ($C) { $$$B }",
    },
    "rust": {
        "tag": "1.83.0", "lang": "rs", "ext": "rs",
        "rare": "DefPathHash", "common": "unwrap", "icase": "tyctxt",
        "regex": r"fn \w+_\w+\(",
        "ast_lit": "$X.unwrap()", "ast_nolit": "if $C { $$$B }",
    },
    "kubernetes": {
        "tag": "v1.31.0", "lang": "go", "ext": "go",
        "rare": "ShouldPodContainersBeTerminating", "common": "fmt.Errorf", "icase": "podspec",
        "regex": r"func \(\w+ \*\w+\) Get\w+\(",
        "ast_lit": "fmt.Errorf($A, $B)", "ast_nolit": "if $C { $$$B }",
    },
    "vscode": {
        "tag": "1.95.0", "lang": "ts", "ext": "ts",
        "rare": "IWorkspaceTrustManagementService", "common": "this._register", "icase": "disposable",
        "regex": r"private readonly _\w+Emitter",
        "ast_lit": "this._register($X)", "ast_nolit": "if ($C) { $$$B }",
    },
    "django": {
        "tag": "5.1", "lang": "py", "ext": "py",
        "rare": "get_prefetch_querysets", "common": "self.assertEqual", "icase": "queryset",
        "regex": r"def test_\w+_\w+\(self\)",
        "ast_lit": "self.assertEqual($A, $B)", "ast_nolit": "if $C:\n    $$$B",
    },
    "cpython": {
        "tag": "v3.13.0", "lang": "c", "ext": "c",
        "rare": "PyUnstable_Object_ClearWeakRefsNoCallbacks", "common": "Py_DECREF", "icase": "pyobject",
        "regex": r"Py\w+_Check\(",
        "ast_lit": "Py_DECREF($X);", "ast_nolit": "if ($C) { $$$B }",
    },
    "spring-framework": {
        "tag": "v6.1.14", "lang": "java", "ext": "java",
        "rare": "BeanDefinitionRegistryPostProcessor", "common": "Assert.notNull", "icase": "beanfactory",
        "regex": r"public \w+ get\w+Bean\(",
        "ast_lit": "Assert.notNull($A, $B)", "ast_nolit": "if ($C) { $$$B }",
    },
}

NO_HIT = "zqxjkvbnm_no_such_token"
NOLIT_REGEX = r"\w+\s*=\s*\d{6,}"  # no 3-byte literal: the index cannot narrow it


def tool(name: str) -> str | None:
    p = TOOLS / name
    return str(p) if p.exists() else shutil.which(name)


def run(cmd: list[str], cwd: Path | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=cwd, capture_output=True, stdin=subprocess.DEVNULL)


def version(cmd: list[str]) -> str:
    try:
        out = run(cmd)
        return (out.stdout or out.stderr).decode(errors="replace").splitlines()[0].strip()
    except (OSError, IndexError):
        return "n/a"


def hyperfine(cmds: dict[str, list[str]], cwd: Path, runs: int) -> dict[str, dict]:
    """Time each command; returns name -> {median, mean, stddev, min, max} in ms."""
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as f:
        out = f.name
    # --output=pipe: tools must really write their output (grep/ugrep stop at the
    # first match when stdout is /dev/null, which would not be a fair comparison).
    args = ["hyperfine", "-N", "--output", "pipe", "--warmup", "1", "--runs", str(runs),
            "--export-json", out, "-i"]
    for name, c in cmds.items():
        args += ["--command-name", name, " ".join(shlex_quote(x) for x in c)]
    p = subprocess.run(args, cwd=cwd, capture_output=True, stdin=subprocess.DEVNULL)
    if p.returncode != 0:
        print(p.stderr.decode(errors="replace")[-2000:], file=sys.stderr)
        return {}
    data = json.loads(Path(out).read_text())
    os.unlink(out)
    res = {}
    for name, r in zip(cmds, data["results"]):
        if "summary" in r:  # hyperfine >= 2
            t = r["summary"]["time_wall_clock"]
            res[name] = {k: round(t[k] * 1000, 2) for k in ("median", "mean", "stddev", "min", "max")}
            mem = r["summary"].get("memory_peak_resident")
            if mem:
                res[name]["peak_rss_mib"] = round(mem["median"] / 2**20, 1)
        else:  # hyperfine 1.x
            res[name] = {k: round(r[k] * 1000, 2) for k in ("median", "mean", "stddev", "min", "max")}
    return res


def shlex_quote(s: str) -> str:
    import shlex

    return shlex.quote(s)


def peak_rss(cmd: list[str], cwd: Path | None = None) -> tuple[float, float, subprocess.CompletedProcess]:
    """(wall seconds, peak RSS MiB) via /usr/bin/time -l (macOS) or -v (Linux)."""
    flag = "-l" if sys.platform == "darwin" else "-v"
    t0 = dt.datetime.now()
    p = subprocess.run(["/usr/bin/time", flag, *cmd], cwd=cwd, capture_output=True, stdin=subprocess.DEVNULL)
    wall = (dt.datetime.now() - t0).total_seconds()
    err = p.stderr.decode(errors="replace")
    m = re.search(r"(\d+)\s+maximum resident set size", err)  # bytes on macOS
    if m:
        rss = int(m.group(1)) / 2**20
    else:
        m = re.search(r"Maximum resident set size \(kbytes\): (\d+)", err)
        rss = int(m.group(1)) / 1024 if m else float("nan")
    return wall, rss, p


def du_mib(p: Path) -> float:
    return sum(f.stat().st_size for f in p.rglob("*") if f.is_file()) / 2**20


def corpus_stats(path: Path) -> dict:
    files = run(["git", "ls-files"], cwd=path).stdout.decode(errors="replace").splitlines()
    size = sum((path / f).stat().st_size for f in files if (path / f).is_file())
    return {"files": len(files), "mib": round(size / 2**20, 1)}


def bench_corpus(name: str, cfg: dict, runs: int, skip_index: bool) -> dict:
    path = CORPORA_DIR / name
    lidx = CACHE / "lancet" / name
    zidx = CACHE / "zoekt" / name
    rg, ug, zo, zi, fd, sg = (tool(t) for t in ("rg", "ugrep", "zoekt", "zoekt-index", "fd", "ast-grep"))
    L = str(LANCET)
    out: dict = {"tag": cfg["tag"], "lang": cfg["lang"], **corpus_stats(path)}
    print(f"\n== {name} @ {cfg['tag']}: {out['files']} files, {out['mib']} MiB", flush=True)

    # ---- index build
    if not skip_index or not lidx.exists():
        shutil.rmtree(lidx, ignore_errors=True)
        wall, rss, p = peak_rss([L, "index", ".", "--index-dir", str(lidx)], cwd=path)
        rep = json.loads(p.stdout or b"{}")
        noop = hyperfine({"noop": [L, "index", ".", "--index-dir", str(lidx)]}, path, 3).get("noop", {})
        out["lancet_index"] = {
            "build_s": round(wall, 2), "peak_rss_mib": round(rss), "size_mib": round(du_mib(lidx), 1),
            "segments": rep.get("segments"), "noop_sync_ms": noop.get("median"),
        }
        print(f"   lancet index: {out['lancet_index']}", flush=True)
        if zi:
            shutil.rmtree(zidx, ignore_errors=True)
            zidx.mkdir(parents=True)
            wall, rss, p = peak_rss([zi, "-index", str(zidx), "-disable_ctags", str(path)])
            out["zoekt_index"] = {"build_s": round(wall, 2), "peak_rss_mib": round(rss), "size_mib": round(du_mib(zidx), 1),
                                  "ok": p.returncode == 0}
            print(f"   zoekt index:  {out['zoekt_index']}", flush=True)

    ix = ["--index-dir", str(lidx)]
    lit = {"rare": cfg["rare"], "common": cfg["common"], "no_hit": NO_HIT}

    # ---- text queries: (label, rg-style args, literal?)
    queries = [
        ("rare literal", ["-n", "-F", cfg["rare"]], cfg["rare"]),
        ("common identifier", ["-n", "-F", cfg["common"]], cfg["common"]),
        ("ignore case", ["-n", "-i", cfg["icase"]], None),
        ("word", ["-n", "-w", cfg["common"]], None),
        ("regex with literal", ["-n", cfg["regex"]], None),
        ("regex, no literal", ["-n", NOLIT_REGEX], None),
        ("no hit", ["-n", "-F", NO_HIT], NO_HIT),
        ("count -c", ["-c", "-F", cfg["common"]], None),
    ]
    out["text"] = []
    for label, args, literal in queries:
        ref = run([rg, "--sort", "path", *args, "."], cwd=path)
        rec: dict = {"query": label, "args": args, "lines": ref.stdout.count(b"\n"), "rg_exit": ref.returncode}
        # Correctness: every lancet mode must reproduce ripgrep byte for byte.
        rec["identical"] = {}
        for mode, extra in (("auto", ix), ("trust", [*ix, "--trust-index"]), ("no-index", ["--no-index"])):
            got = run([L, "rg", *args, *extra, "."], cwd=path)
            rec["identical"][mode] = (got.stdout == ref.stdout and got.returncode == ref.returncode)
        cmds = {
            "rg": [rg, *args, "."],
            "lancet rg": [L, "rg", *args, *ix, "."],
            "lancet rg --trust-index": [L, "rg", *args, *ix, "--trust-index", "."],
            "lancet rg --no-index": [L, "rg", *args, "--no-index", "."],
        }
        if ug and "-c" not in args:  # ugrep -c also prints `file:0` for every non-matching file
            cmds["ugrep"] = [ug, "-rI", "--ignore-files", *args, "."]
        if literal:
            cmds["git grep"] = ["git", "grep", "-n", "-I", "-F", literal]
            if zo:
                cmds["zoekt"] = [zo, "-index_dir", str(zidx), f"case:yes {json.dumps(literal)}"]
        # Competitors must produce the same lines as ripgrep (zoekt and git grep print
        # one line per match too); a tool that disagrees is flagged, not trusted.
        rec["lines_by_tool"] = {}
        for t, c in cmds.items():
            if t.startswith("lancet"):
                continue
            p = run(c, cwd=path)
            rec["lines_by_tool"][t] = p.stdout.count(b"\n")
        rec["ms"] = hyperfine(cmds, path, runs)
        bad = [m for m, ok in rec["identical"].items() if not ok]
        bad += [f"{t}:{n}" for t, n in rec["lines_by_tool"].items() if n != rec["lines"]]
        print(f"   {label:<20} {rec['lines']:>7} lines  "
              + "  ".join(f"{k}={v.get('median', '?')}" for k, v in rec["ms"].items())
              + (f"  MISMATCH:{bad}" if bad else ""), flush=True)
        out["text"].append(rec)

    # ---- AST
    out["ast"] = []
    for label, pat in (("ast, literal", cfg["ast_lit"]), ("ast, no literal", cfg["ast_nolit"])):
        la = run([L, "ast", pat, ".", "--lang", cfg["lang"], *ix, "--json"], cwd=path)
        try:
            lj = json.loads(la.stdout)
            lancet_n, status = lj.get("matched"), lj.get("status")
        except json.JSONDecodeError:
            lancet_n, status = None, "error"
        sg_n = None
        if sg:
            s = run([sg, "run", "-p", pat, "-l", cfg["lang"], "--json=stream", "."], cwd=path)
            sg_n = s.stdout.count(b"\n") if s.returncode in (0, 1) else None
        cmds = {
            "lancet ast": [L, "ast", pat, ".", "--lang", cfg["lang"], *ix],
            "lancet ast --trust-index": [L, "ast", pat, ".", "--lang", cfg["lang"], *ix, "--trust-index"],
            "lancet ast --no-index": [L, "ast", pat, ".", "--lang", cfg["lang"], "--no-index"],
        }
        if sg:
            cmds["ast-grep"] = [sg, "run", "-p", pat, "-l", cfg["lang"], "."]
        rec = {"query": label, "pattern": pat, "lancet_matches": lancet_n, "lancet_status": status,
               "ast_grep_matches": sg_n, "ms": hyperfine(cmds, path, max(3, runs // 2))}
        print(f"   {label:<20} lancet={lancet_n} ast-grep={sg_n}  "
              + "  ".join(f"{k}={v.get('median', '?')}" for k, v in rec["ms"].items()), flush=True)
        out["ast"].append(rec)

    # ---- file discovery
    cmds = {"lancet fd": [L, "fd", "-t", "f", "-e", cfg["ext"], "."],
            "lancet fd --trust-index": [L, "fd", "--trust-index", *ix, "-t", "f", "-e", cfg["ext"], "."]}
    if fd:
        cmds["fd"] = [fd, "-t", "f", "-e", cfg["ext"], "."]
    out["fd"] = {"query": f"-e {cfg['ext']}", "ms": hyperfine(cmds, path, runs)}
    print("   fd -e " + cfg["ext"] + "  " + "  ".join(f"{k}={v.get('median', '?')}" for k, v in out["fd"]["ms"].items()), flush=True)
    return out


def summary_md(res: dict) -> str:
    lines = [f"# Benchmark suite — {res['stamp']}", "",
             f"Machine: {res['env']['machine']}. Warm cache, median of {res['env']['runs']} runs (hyperfine -N).", "",
             "Tools: " + ", ".join(f"{k} `{v}`" for k, v in res["env"]["tools"].items()), ""]
    for name, c in res["corpora"].items():
        lines += [f"## {name} @ {c['tag']} — {c['files']:,} files, {c['mib']:,} MiB", ""]
        if "lancet_index" in c:
            li = c["lancet_index"]
            z = c.get("zoekt_index", {})
            lines += [f"Index: lancet {li['build_s']} s, {li['peak_rss_mib']} MiB peak, {li['size_mib']} MiB on disk, "
                      f"no-op sync {li['noop_sync_ms']} ms" + (f" · zoekt {z.get('build_s')} s, {z.get('peak_rss_mib')} MiB peak, "
                      f"{z.get('size_mib')} MiB" if z else ""), ""]
        tools = ["rg", "lancet rg", "lancet rg --trust-index", "lancet rg --no-index", "ugrep", "git grep", "zoekt"]
        lines += ["| query | lines | " + " | ".join(tools) + " | = rg |", "|---|---:|" + "---:|" * len(tools) + "---|"]
        for q in c["text"]:
            ok = "✓" if all(q["identical"].values()) else "✗ " + ",".join(m for m, v in q["identical"].items() if not v)
            diff = [f"{t} {n:,}" for t, n in q.get("lines_by_tool", {}).items() if n != q["lines"]]
            if diff:
                ok += " (other tools: " + ", ".join(diff) + " lines)"
            lines.append(f"| {q['query']} | {q['lines']:,} | " + " | ".join(
                f"{q['ms'][t]['median']:.0f}" if t in q["ms"] else "—" for t in tools) + f" | {ok} |")
        lines += ["", "| AST | lancet / ast-grep matches | lancet ast | --trust-index | --no-index | ast-grep |", "|---|---|---:|---:|---:|---:|"]
        for a in c["ast"]:
            ms = a["ms"]
            g = lambda k: f"{ms[k]['median']:.0f}" if k in ms else "—"  # noqa: E731
            lines.append(f"| `{a['pattern']}` | {a['lancet_matches']} / {a['ast_grep_matches']} | {g('lancet ast')} | "
                         f"{g('lancet ast --trust-index')} | {g('lancet ast --no-index')} | {g('ast-grep')} |")
        fdm = c["fd"]["ms"]
        lines += ["", f"File discovery `{c['fd']['query']}`: " + ", ".join(f"{k} {v['median']:.0f} ms" for k, v in fdm.items()), ""]
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", nargs="*", default=None)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--skip-index", action="store_true", help="reuse existing indexes")
    args = ap.parse_args()
    names = args.corpus or [n for n in CORPORA if (CORPORA_DIR / n).exists()]
    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M")
    env = {
        "machine": f"{platform.machine()} {platform.platform()}",
        "runs": args.runs,
        "tools": {
            "lancet": version([str(LANCET), "--version"]),
            "ripgrep": version([tool("rg") or "rg", "--version"]),
            "ugrep": version([tool("ugrep") or "ugrep", "--version"]),
            "git": version(["git", "--version"]),
            "fd": version([tool("fd") or "fd", "--version"]),
            "ast-grep": version([tool("ast-grep") or "ast-grep", "--version"]),
            "zoekt": "latest (go install)",
            "hyperfine": version(["hyperfine", "--version"]),
        },
    }
    res = {"stamp": stamp, "env": env, "corpora": {}}
    RESULTS.mkdir(parents=True, exist_ok=True)
    for n in names:
        res["corpora"][n] = bench_corpus(n, CORPORA[n], args.runs, args.skip_index)
        (RESULTS / f"suite-{stamp}.json").write_text(json.dumps(res, indent=1))
    (RESULTS / "SUITE.md").write_text(summary_md(res))
    print(f"\nwrote bench/results/suite-{stamp}.json and bench/results/SUITE.md")
    return 0


if __name__ == "__main__":
    sys.exit(main())
