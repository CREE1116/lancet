#!/usr/bin/env python3
"""Head-to-head benchmark against agent/indexed search competitors:
  - lancet (daemonless trigram index + exact verification + AST)
  - ripgrep 15.2 (no-index baseline)
  - ast-grep 0.51.3 (AST baseline)
  - Microsoft tgrep 1.2.9 (serverless & `tgrep serve` daemon)
  - grix 0.6.0 (auto-refresh & `--no-auto-index`)
  - CortexKit aft 0.68.0 (persistent stdio JSON-RPC resident process)
  - tensor-grep 1.142.0 (Python CLI + ast-grep/rg wrapper on arm64 macOS)
  - probe 0.6.0 (indexless AST-block semantic search)

Usage:
    python3 bench/competitors_agents.py
"""

from __future__ import annotations

import json
import os
import platform
import re
import shlex
import shutil
import signal
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORPORA = ROOT / "bench" / "corpora"
TOOLS = ROOT / "bench" / ".cache" / "tools"
TG_VENV_BIN = ROOT / "bench" / ".cache" / "tg-venv" / "bin"
RESULTS = ROOT / "bench" / "results"
SUITE = json.loads((RESULTS / "suite-final.json").read_text(encoding="utf-8"))

LANCET = str(ROOT / "target" / "release" / "lancet")
RG = shutil.which("rg") or "rg"
AST_GREP = str(TOOLS / "ast-grep") if (TOOLS / "ast-grep").exists() else (shutil.which("ast-grep") or "ast-grep")
TGREP = str(TOOLS / "tgrep")
GRIX = str(TOOLS / "grix")
AFT = str(TOOLS / "aft")
PROBE = str(TOOLS / "probe")
TG = str(TG_VENV_BIN / "tg")

TARGET_CORPORA = [
    ("django", "python"),
    ("rust", "rust"),
    ("linux", "c"),
    ("llvm", "cpp"),
]

QUERY_KINDS = [
    "rare literal",
    "common identifier",
    "regex with literal",
    "regex, no literal",
    "no hit",
]


def dir_bytes(p: Path) -> int:
    if not p.exists():
        return 0
    if p.is_file():
        return p.stat().st_size
    return sum(f.stat().st_size for f in p.rglob("*") if f.is_file())


def run_pg(cmd: list[str], cwd: Path | None = None, env: dict | None = None, timeout: int = 180, text: bool = True):
    """Run subprocess in its own process group so timeouts kill grandchildren too."""
    p = subprocess.Popen(
        cmd,
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=text,
        stdin=subprocess.DEVNULL,
        start_new_session=True,
    )
    try:
        out, err = p.communicate(timeout=timeout)
        return p.returncode, out, err, False
    except subprocess.TimeoutExpired:
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        try:
            out, err = p.communicate(timeout=5)
        except Exception:
            out, err = ("" if text else b""), ("" if text else b"")
        return -1, out, err, True


def time_cmd_rss(cmd: list[str], cwd: Path | None = None, env: dict | None = None, timeout: int = 240) -> dict:
    """Run a command under /usr/bin/time -l on macOS to capture wall_s and peak_rss_mib."""
    t0 = time.perf_counter()
    rc, out, err, timed_out = run_pg(["/usr/bin/time", "-l", *cmd], cwd=cwd, env=env, timeout=timeout, text=True)
    if timed_out:
        return {"ok": False, "timeout": True, "wall_s": timeout}
    wall_s = time.perf_counter() - t0
    m = re.search(r"(\d+)\s+maximum resident set size", err)
    rss_mib = round(int(m.group(1)) / (1024 * 1024), 1) if m else None
    return {
        "ok": rc in (0, 1),
        "rc": rc,
        "wall_s": round(wall_s, 3),
        "peak_rss_mib": rss_mib,
        "stdout": out,
        "stderr": err,
    }


def ps_rss_mib(pid: int) -> float | None:
    p = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True)
    if p.returncode != 0 or not p.stdout.strip():
        return None
    return round(int(p.stdout.strip()) / 1024.0, 1)


def hyperfine_median_ms(cmd: list[str], cwd: Path, env: dict | None = None, runs: int = 5, timeout: int = 180) -> dict:
    """Measure median wall-clock ms and peak RSS using hyperfine 2 -N --output pipe."""
    cmd_str = " ".join(shlex.quote(c) for c in cmd)
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as tf:
        jpath = Path(tf.name)
    try:
        rc, _, err, timed_out = run_pg(
            ["hyperfine", "-N", "--output", "pipe", "-i", "--warmup", "1", "--runs", str(runs), "--export-json", str(jpath), cmd_str],
            cwd=cwd,
            env=env,
            timeout=timeout,
            text=True,
        )
        if timed_out:
            return {"ok": False, "timeout": True}
        if rc != 0 or not jpath.exists() or jpath.stat().st_size == 0:
            return {"ok": False, "error": err.strip()[:200]}
        res = json.loads(jpath.read_text(encoding="utf-8"))["results"][0]
        if "summary" in res:
            t = res["summary"]["time_wall_clock"]
            mem = res["summary"].get("memory_peak_resident")
            out = {
                "ok": True,
                "median_ms": round(t["median"] * 1000.0, 2),
                "min_ms": round(t["min"] * 1000.0, 2),
                "max_ms": round(t["max"] * 1000.0, 2),
            }
            if mem:
                out["peak_rss_mib"] = round(mem["median"] / (1024 * 1024), 1)
            return out
        return {
            "ok": True,
            "median_ms": round(res["median"] * 1000.0, 2),
            "min_ms": round(res["min"] * 1000.0, 2),
            "max_ms": round(res["max"] * 1000.0, 2),
        }
    finally:
        jpath.unlink(missing_ok=True)


def count_lines(cmd: list[str], cwd: Path, env: dict | None = None, timeout: int = 60) -> int | None:
    rc, out, _, timed_out = run_pg(cmd, cwd=cwd, env=env, timeout=timeout, text=False)
    if timed_out or rc not in (0, 1):
        return None
    return len(out.splitlines())


def grix_index_path(repo: Path) -> Path | None:
    p = subprocess.run([GRIX, "status", str(repo)], capture_output=True, text=True, stdin=subprocess.DEVNULL)
    m = re.search(r"index:\s+(\S+)", p.stdout)
    return Path(m.group(1)) if m else None


# ---------------------------------------------------------------------------
# AFT JSON-RPC resident harness
# ---------------------------------------------------------------------------


class AftClient:
    def __init__(self, project_root: Path, xdg_data_home: Path):
        self.project_root = project_root.resolve()
        self.xdg_data_home = xdg_data_home
        self._seq = 0
        self.proc = subprocess.Popen(
            [AFT],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
            env={**os.environ, "XDG_DATA_HOME": str(xdg_data_home)},
        )

    def rpc(self, command: str, timeout: float = 180.0, **params) -> dict:
        self._seq += 1
        rid = f"r{self._seq}"
        payload = {"id": rid, "command": command, **params}
        assert self.proc.stdin and self.proc.stdout
        self.proc.stdin.write(json.dumps(payload) + "\n")
        self.proc.stdin.flush()
        deadline = time.perf_counter() + timeout
        while time.perf_counter() < deadline:
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError("aft exited prematurely")
            msg = json.loads(line)
            if msg.get("id") == rid:
                return msg
        raise TimeoutError(f"aft rpc {command} timed out")

    def configure_and_wait(self, timeout: float = 180.0) -> dict:
        t0 = time.perf_counter()
        cfg = self.rpc("configure", harness="runner", project_root=str(self.project_root), timeout=timeout)
        if not cfg.get("success"):
            return {"ok": False, "error": cfg}
        deadline = time.perf_counter() + timeout
        last_status = {}
        while time.perf_counter() < deadline:
            st = self.rpc("status", timeout=30.0)
            last_status = st
            si = st.get("search_index", {})
            if si.get("status") == "ready":
                build_s = time.perf_counter() - t0
                return {
                    "ok": True,
                    "build_s": round(build_s, 3),
                    "files": si.get("files"),
                    "resident_rss_mib": ps_rss_mib(self.proc.pid),
                    "disk_mib": round(dir_bytes(self.xdg_data_home) / (1024 * 1024), 2),
                }
            time.sleep(0.05)
        return {"ok": False, "timeout": True, "last_status": last_status}

    def close(self) -> None:
        try:
            self.proc.terminate()
            self.proc.wait(timeout=5)
        except Exception:
            try:
                self.proc.kill()
            except Exception:
                pass


# ---------------------------------------------------------------------------
# Per-corpus benchmark
# ---------------------------------------------------------------------------


def benchmark_corpus(corpus: str, lang: str) -> dict:
    repo = (CORPORA / corpus).resolve()
    c_suite = SUITE["corpora"][corpus]
    qmap = {q["query"]: q for q in c_suite["text"]}
    ast_case = c_suite["ast"][0]  # ast, literal
    out: dict = {
        "files": c_suite["files"],
        "mib": c_suite["mib"],
        "lang": lang,
        "index": {},
        "text": {},
        "ast": {},
    }

    tg_env = {**os.environ, "PATH": f"{TG_VENV_BIN}:{TOOLS}:{os.environ.get('PATH', '')}"}

    # Clean any stale indexes from prior runs
    shutil.rmtree(repo / ".lancet", ignore_errors=True)
    shutil.rmtree(repo / ".tgrep", ignore_errors=True)
    subprocess.run([GRIX, "forget", str(repo)], capture_output=True, stdin=subprocess.DEVNULL)

    try:
        # 1) Index builds
        print(f"\n[{corpus}] Index builds...")
        # lancet
        r_lan = time_cmd_rss([LANCET, "index", "."], cwd=repo)
        out["index"]["lancet"] = {
            "build_s": r_lan["wall_s"],
            "peak_rss_mib": r_lan["peak_rss_mib"],
            "size_mib": round(dir_bytes(repo / ".lancet") / (1024 * 1024), 2),
        }

        # tgrep
        r_tg = time_cmd_rss([TGREP, "index", "."], cwd=repo)
        out["index"]["tgrep"] = {
            "build_s": r_tg["wall_s"],
            "peak_rss_mib": r_tg["peak_rss_mib"],
            "size_mib": round(dir_bytes(repo / ".tgrep") / (1024 * 1024), 2),
        }

        # grix
        r_gx = time_cmd_rss([GRIX, "index", "."], cwd=repo)
        grix_path = grix_index_path(repo)
        out["index"]["grix"] = {
            "build_s": r_gx["wall_s"],
            "peak_rss_mib": r_gx["peak_rss_mib"],
            "size_mib": round(dir_bytes(grix_path) / (1024 * 1024), 2) if grix_path else None,
        }

        # 2) Serverless text search benchmarks (rg, lancet, tgrep serverless, grix, tensor-grep, probe)
        print(f"[{corpus}] Text queries (CLI / serverless)...", flush=True)
        for qk in QUERY_KINDS:
            print(f"  [{corpus}] {qk}...", flush=True)
            q = qmap[qk]
            args = q["args"]  # e.g. ['-n', '-F', 'PM_RESUME']
            pat = args[-1]
            is_fixed = "-F" in args

            row: dict = {"args": args, "rg_lines": q["lines"], "tools": {}}

            # rg baseline
            m_rg = hyperfine_median_ms([RG, *args, "."], cwd=repo)
            m_rg["lines"] = count_lines([RG, *args, "."], cwd=repo)
            row["tools"]["rg"] = m_rg

            # lancet rg (auto) & --trust-index
            for tname, extra in (("lancet_auto", []), ("lancet_trust", ["--trust-index"])):
                cmd = [LANCET, "rg", *extra, *args, "."]
                m = hyperfine_median_ms(cmd, cwd=repo)
                m["lines"] = count_lines(cmd, cwd=repo)
                row["tools"][tname] = m

            # tgrep (serverless, index already built)
            tg_args = ["-n", "-F", pat] if is_fixed else ["-n", pat]
            m_tgr = hyperfine_median_ms([TGREP, *tg_args, "."], cwd=repo)
            m_tgr["lines"] = count_lines([TGREP, *tg_args, "."], cwd=repo)
            row["tools"]["tgrep_serverless"] = m_tgr

            # grix (auto & --no-auto-index)
            gx_args = ["-F", pat] if is_fixed else [pat]
            for tname, extra in (("grix_auto", []), ("grix_trust", ["--no-auto-index"])):
                cmd = [GRIX, *extra, *gx_args, "."]
                m = hyperfine_median_ms(cmd, cwd=repo)
                m["lines"] = count_lines(cmd, cwd=repo)
                row["tools"][tname] = m

            # tensor-grep (skip llvm if too slow; guard with timeout)
            if corpus in ("django", "rust", "linux"):
                cmd_tgp = [TG, "search", *args, "."]
                m_tgp = hyperfine_median_ms(cmd_tgp, cwd=repo, env=tg_env, runs=3, timeout=90)
                m_tgp["lines"] = count_lines(cmd_tgp, cwd=repo, env=tg_env, timeout=45)
                row["tools"]["tensor_grep"] = m_tgp

            # probe (semantic AST-block search — skip common identifier on linux/llvm where it times out)
            if qk == "rare literal" or (qk == "common identifier" and corpus in ("django", "rust")):
                cmd_pr = [PROBE, "search", "-e", pat, "."]
                m_pr = hyperfine_median_ms(cmd_pr, cwd=repo, runs=3, timeout=60)
                rc_p, p_out, _, to_p = run_pg(cmd_pr, cwd=repo, timeout=45, text=True)
                if not to_p and rc_p == 0:
                    m_res = re.search(r"Found (\d+) search results", p_out)
                    m_pr["blocks"] = int(m_res.group(1)) if m_res else None
                row["tools"]["probe"] = m_pr

            out["text"][qk] = row

        # 3) tgrep serve (persistent daemon)
        print(f"[{corpus}] tgrep serve...")
        serve_proc = subprocess.Popen(
            [TGREP, "serve", "."],
            cwd=repo,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            stdin=subprocess.DEVNULL,
        )
        try:
            ready = False
            t0 = time.perf_counter()
            while time.perf_counter() - t0 < 60:
                st = subprocess.run([TGREP, "status", "."], cwd=repo, capture_output=True, text=True, stdin=subprocess.DEVNULL)
                if "serving" in st.stdout.lower() or "ready" in st.stdout.lower() or "running" in st.stdout.lower():
                    ready = True
                    break
                time.sleep(0.1)
            out["index"]["tgrep_serve"] = {
                "ready": ready,
                "ready_s": round(time.perf_counter() - t0, 3),
                "resident_rss_mib": ps_rss_mib(serve_proc.pid),
            }
            for qk in QUERY_KINDS:
                args = qmap[qk]["args"]
                pat = args[-1]
                tg_args = ["-n", "-F", pat] if "-F" in args else ["-n", pat]
                m = hyperfine_median_ms([TGREP, *tg_args, "."], cwd=repo)
                m["lines"] = count_lines([TGREP, *tg_args, "."], cwd=repo)
                out["text"][qk]["tools"]["tgrep_serve"] = m
        finally:
            serve_proc.terminate()
            try:
                serve_proc.wait(timeout=5)
            except Exception:
                serve_proc.kill()
            time.sleep(0.2)

        # 4) AFT resident stdio process
        print(f"[{corpus}] AFT resident...")
        aft_xdg = Path(tempfile.mkdtemp(prefix=f"aft_{corpus}_"))
        aft = AftClient(repo, aft_xdg)
        try:
            cfg = aft.configure_and_wait(timeout=240.0)
            out["index"]["aft"] = cfg
            if cfg.get("ok"):
                for qk in QUERY_KINDS:
                    args = qmap[qk]["args"]
                    pat = args[-1]
                    aft_pat = re.escape(pat) if "-F" in args else pat
                    # Warmup + 5 measured runs
                    _ = aft.rpc("grep", pattern=aft_pat, max_results=100000)
                    times_ms = []
                    last_res = {}
                    for _ in range(5):
                        t0 = time.perf_counter()
                        last_res = aft.rpc("grep", pattern=aft_pat, max_results=100000)
                        times_ms.append((time.perf_counter() - t0) * 1000.0)
                    times_ms.sort()
                    out["text"][qk]["tools"]["aft_resident"] = {
                        "ok": last_res.get("success", False),
                        "median_ms": round(times_ms[2], 2),
                        "min_ms": round(times_ms[0], 2),
                        "max_ms": round(times_ms[-1], 2),
                        "total_matches": last_res.get("total_matches"),
                        "rendered_matches": last_res.get("rendered_matches"),
                        "truncated": last_res.get("truncated"),
                        "index_status": last_res.get("index_status"),
                    }
        finally:
            aft.close()
            shutil.rmtree(aft_xdg, ignore_errors=True)

        # 5) AST structural search (ast_literal)
        print(f"[{corpus}] AST structural search ({ast_case['pattern']})...")
        pat_ast = ast_case["pattern"]
        out["ast"]["pattern"] = pat_ast

        # lancet ast
        m_la = hyperfine_median_ms([LANCET, "ast", pat_ast, ".", "--lang", lang], cwd=repo, runs=3)
        p_la = subprocess.run([LANCET, "ast", pat_ast, ".", "--lang", lang, "--json"], cwd=repo, capture_output=True, text=True, stdin=subprocess.DEVNULL)
        try:
            m_la["matches"] = json.loads(p_la.stdout).get("matched")
        except Exception:
            pass
        out["ast"]["lancet_ast"] = m_la

        # ast-grep
        m_ag = hyperfine_median_ms([AST_GREP, "run", "-p", pat_ast, "-l", lang, "."], cwd=repo, runs=3)
        m_ag["matches"] = ast_case["ast_grep_matches"]
        out["ast"]["ast_grep"] = m_ag

        # tensor-grep run
        if corpus in ("django", "rust", "linux"):
            m_tgrun = hyperfine_median_ms([TG, "run", "--lang", lang, pat_ast, "."], cwd=repo, env=tg_env, runs=3, timeout=120)
            out["ast"]["tensor_grep_run"] = m_tgrun

        # aft ast_search (reusing a clean resident instance)
        aft_xdg2 = Path(tempfile.mkdtemp(prefix=f"aft_ast_{corpus}_"))
        aft2 = AftClient(repo, aft_xdg2)
        try:
            cfg2 = aft2.configure_and_wait(timeout=240.0)
            if cfg2.get("ok"):
                aft_lang = {"python": "python", "rust": "rust", "c": "c", "cpp": "cpp"}[lang]
                _ = aft2.rpc("ast_search", pattern=pat_ast, lang=aft_lang)
                times_ms = []
                last_r = {}
                for _ in range(3):
                    t0 = time.perf_counter()
                    last_r = aft2.rpc("ast_search", pattern=pat_ast, lang=aft_lang, timeout=180.0)
                    times_ms.append((time.perf_counter() - t0) * 1000.0)
                times_ms.sort()
                out["ast"]["aft_ast_search"] = {
                    "ok": last_r.get("success", False),
                    "median_ms": round(times_ms[1], 2),
                    "total_matches": last_r.get("total_matches"),
                }
        finally:
            aft2.close()
            shutil.rmtree(aft_xdg2, ignore_errors=True)

    finally:
        # Always clean temporary indexes from corpus
        shutil.rmtree(repo / ".tgrep", ignore_errors=True)
        subprocess.run([GRIX, "forget", str(repo)], capture_output=True, stdin=subprocess.DEVNULL)

    return out


# ---------------------------------------------------------------------------
# Freshness test across indexed competitors (Scenario G protocol)
# ---------------------------------------------------------------------------


def benchmark_freshness() -> dict:
    print("\n[freshness] Running 20-file post-edit freshness test on Django copy...")
    src = CORPORA / "django"
    tmp_root = Path(tempfile.mkdtemp(prefix="freshness_"))
    repo = tmp_root / "django"
    shutil.copytree(src, repo, ignore=shutil.ignore_patterns(".git", ".lancet", ".tgrep"))

    files = sorted(repo.rglob("*.py"))[:400:20]  # 20 files
    assert len(files) == 20
    for i, f in enumerate(files):
        f.write_text(f.read_text(encoding="utf-8") + f"\nOLD_TOKEN_{i:02d} = {i}\n", encoding="utf-8")

    results: dict = {"edited_files": len(files), "tools": {}}
    aft_xdg = Path(tempfile.mkdtemp(prefix="aft_fresh_"))
    serve_proc = None
    aft = None

    try:
        # Build all indexes before editing
        subprocess.run([LANCET, "index", "."], cwd=repo, check=True, capture_output=True, stdin=subprocess.DEVNULL)
        subprocess.run([TGREP, "index", "."], cwd=repo, check=True, capture_output=True, stdin=subprocess.DEVNULL)
        subprocess.run([GRIX, "index", "."], cwd=repo, check=True, capture_output=True, stdin=subprocess.DEVNULL)

        serve_proc = subprocess.Popen([TGREP, "serve", "."], cwd=repo, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, stdin=subprocess.DEVNULL)
        t0 = time.perf_counter()
        while time.perf_counter() - t0 < 30:
            st = subprocess.run([TGREP, "status", "."], cwd=repo, capture_output=True, text=True, stdin=subprocess.DEVNULL)
            if "serving" in st.stdout.lower() or "ready" in st.stdout.lower() or "running" in st.stdout.lower():
                break
            time.sleep(0.1)

        aft = AftClient(repo, aft_xdg)
        aft.configure_and_wait(timeout=60.0)
        time.sleep(0.5)  # allow AFT FSEvents watcher to settle before editing

        # Perform edits: replace OLD_TOKEN_xx -> FRESH_TOKEN_xx
        for i, f in enumerate(files):
            f.write_text(f.read_text(encoding="utf-8").replace(f"OLD_TOKEN_{i:02d}", f"FRESH_TOKEN_{i:02d}"), encoding="utf-8")

        # 0ms immediate queries
        def cli_files(cmd: list[str]) -> int:
            p = subprocess.run(cmd, cwd=repo, capture_output=True, text=True, stdin=subprocess.DEVNULL)
            return len([ln for ln in p.stdout.splitlines() if ln.strip()])

        results["tools"]["lancet_auto"] = {
            "new_0ms": cli_files([LANCET, "rg", "-l", "-F", "FRESH_TOKEN_", "."]),
            "old_0ms": cli_files([LANCET, "rg", "-l", "-F", "OLD_TOKEN_", "."]),
        }
        results["tools"]["lancet_trust"] = {
            "new_0ms": cli_files([LANCET, "rg", "--trust-index", "-l", "-F", "FRESH_TOKEN_", "."]),
            "old_0ms": cli_files([LANCET, "rg", "--trust-index", "-l", "-F", "OLD_TOKEN_", "."]),
        }
        results["tools"]["grix_auto"] = {
            "new_0ms": cli_files([GRIX, "-l", "-F", "FRESH_TOKEN_", "."]),
            "old_0ms": cli_files([GRIX, "-l", "-F", "OLD_TOKEN_", "."]),
        }
        results["tools"]["tgrep_serve"] = {
            "new_0ms": cli_files([TGREP, "-l", "-F", "FRESH_TOKEN_", "."]),
            "old_0ms": cli_files([TGREP, "-l", "-F", "OLD_TOKEN_", "."]),
        }

        def aft_files(pat: str) -> int:
            r = aft.rpc("grep", pattern=pat, max_results=1000)
            return r.get("files_with_matches", 0)

        results["tools"]["aft_resident"] = {
            "new_0ms": aft_files("FRESH_TOKEN_"),
            "old_0ms": aft_files("OLD_TOKEN_"),
        }

        # Re-check daemon tools after 1.5s watcher settle
        time.sleep(1.5)
        results["tools"]["tgrep_serve"]["new_1500ms"] = cli_files([TGREP, "-l", "-F", "FRESH_TOKEN_", "."])
        results["tools"]["tgrep_serve"]["old_1500ms"] = cli_files([TGREP, "-l", "-F", "OLD_TOKEN_", "."])
        results["tools"]["aft_resident"]["new_1500ms"] = aft_files("FRESH_TOKEN_")
        results["tools"]["aft_resident"]["old_1500ms"] = aft_files("OLD_TOKEN_")

        # Serverless tgrep (stop server first so it queries on-disk index directly)
        serve_proc.terminate()
        serve_proc.wait(timeout=5)
        serve_proc = None
        results["tools"]["tgrep_serverless"] = {
            "new": cli_files([TGREP, "-l", "-F", "FRESH_TOKEN_", "."]),
            "old": cli_files([TGREP, "-l", "-F", "OLD_TOKEN_", "."]),
        }

    finally:
        if serve_proc is not None:
            serve_proc.terminate()
            try:
                serve_proc.wait(timeout=5)
            except Exception:
                serve_proc.kill()
        if aft is not None:
            aft.close()
        subprocess.run([GRIX, "forget", str(repo)], capture_output=True, stdin=subprocess.DEVNULL)
        shutil.rmtree(tmp_root, ignore_errors=True)
        shutil.rmtree(aft_xdg, ignore_errors=True)

    return results


# ---------------------------------------------------------------------------
# Markdown summary generator
# ---------------------------------------------------------------------------


def write_markdown(data: dict) -> None:
    lines = [
        "# Agent & Indexed-Search Competitor Benchmark",
        "",
        f"Machine: `{data['env']['machine']}` · Date: `{data['env']['timestamp']}`",
        "",
        "Tools:",
        f"- `lancet`: `{data['env']['versions']['lancet']}` (daemonless trigram index + exact evaluator)",
        f"- `tgrep`: `{data['env']['versions']['tgrep']}` (Microsoft; serverless on-disk index + `tgrep serve` daemon)",
        f"- `grix`: `{data['env']['versions']['grix']}` (auto-refreshing trigram index + `--no-auto-index`)",
        f"- `aft`: `{data['env']['versions']['aft']}` (CortexKit; persistent stdio JSON-RPC process + watcher)",
        f"- `tensor-grep`: `{data['env']['versions']['tensor-grep']}` (Python CLI wrapping `rg`/`ast-grep`; no arm64 macOS native binary on PyPI)",
        f"- `probe`: `{data['env']['versions']['probe']}` (indexless AST-block BM25 search)",
        "",
        "## 1. Index build time, disk footprint, and memory",
        "",
        "| Corpus | Files | lancet (build / disk / peak RSS) | tgrep (build / disk / peak RSS) | grix (build / disk / peak RSS) | aft (ready / disk / resident RSS) | tgrep serve RSS |",
        "|---|---:|---|---|---|---|---:|",
    ]
    for c, _ in TARGET_CORPORA:
        cd = data["corpora"][c]
        ix = cd["index"]
        li = ix["lancet"]
        ti = ix["tgrep"]
        gi = ix["grix"]
        ai = ix.get("aft", {})
        ts = ix.get("tgrep_serve", {})
        aft_str = (
            f"{ai.get('build_s')}s / {ai.get('disk_mib')} MB / {ai.get('resident_rss_mib')} MB"
            if ai.get("ok")
            else "timeout/err"
        )
        lines.append(
            f"| {c} | {cd['files']:,} | {li['build_s']}s / {li['size_mib']} MB / {li['peak_rss_mib']} MB | "
            f"{ti['build_s']}s / {ti['size_mib']} MB / {ti['peak_rss_mib']} MB | "
            f"{gi['build_s']}s / {gi['size_mib']} MB / {gi['peak_rss_mib']} MB | "
            f"{aft_str} | {ts.get('resident_rss_mib')} MB |"
        )

    lines.extend(["", "## 2. Text search latency (ms, median of 5 runs)", ""])
    for c, _ in TARGET_CORPORA:
        cd = data["corpora"][c]
        lines.extend([
            f"### {c} ({cd['files']:,} files)",
            "",
            "| Query | rg | lancet auto | lancet trust | tgrep (disk) | tgrep serve | grix auto | grix trust | aft (resident) | tensor-grep |",
            "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
        ])
        for qk in QUERY_KINDS:
            t = cd["text"][qk]["tools"]
            def fm(k):
                v = t.get(k, {})
                return f"{v['median_ms']:.1f}" if v.get("ok") else "—"
            lines.append(
                f"| {qk} | {fm('rg')} | {fm('lancet_auto')} | {fm('lancet_trust')} | {fm('tgrep_serverless')} | "
                f"{fm('tgrep_serve')} | {fm('grix_auto')} | {fm('grix_trust')} | {fm('aft_resident')} | {fm('tensor_grep')} |"
            )
        lines.append("")

    lines.extend([
        "## 3. AST structural search (`ast, literal` pattern, ms)",
        "",
        "| Corpus | Pattern | ast-grep | lancet ast | aft `ast_search` | tensor-grep `tg run` |",
        "|---|---|---:|---:|---:|---:|",
    ])
    for c, _ in TARGET_CORPORA:
        a = data["corpora"][c]["ast"]
        def fa(k):
            v = a.get(k, {})
            if not v or not v.get("ok"):
                return "—"
            m = v.get("matches") if "matches" in v else v.get("total_matches")
            return f"{v['median_ms']:.1f} ms ({m:,})" if m is not None else f"{v['median_ms']:.1f} ms"
        lines.append(f"| {c} | `{a['pattern']}` | {fa('ast_grep')} | {fa('lancet_ast')} | {fa('aft_ast_search')} | {fa('tensor_grep_run')} |")

    fr = data["freshness"]["tools"]
    lines.extend([
        "",
        "## 4. Post-edit freshness (20 edited `.py` files in Django)",
        "",
        "| Tool / Mode | Finds new token (0 ms) | Still finds old token (0 ms) | After 1.5 s settle (new / old) |",
        "|---|---:|---:|---|",
        f"| `lancet rg` (auto) | {fr['lancet_auto']['new_0ms']}/20 | {fr['lancet_auto']['old_0ms']}/20 | — |",
        f"| `lancet rg --trust-index` | {fr['lancet_trust']['new_0ms']}/20 | {fr['lancet_trust']['old_0ms']}/20 | — |",
        f"| `grix` (auto) | {fr['grix_auto']['new_0ms']}/20 | {fr['grix_auto']['old_0ms']}/20 | — |",
        f"| `tgrep` (serverless) | {fr['tgrep_serverless']['new']}/20 | {fr['tgrep_serverless']['old']}/20 | — |",
        f"| `tgrep serve` (daemon) | {fr['tgrep_serve']['new_0ms']}/20 | {fr['tgrep_serve']['old_0ms']}/20 | {fr['tgrep_serve']['new_1500ms']}/20 / {fr['tgrep_serve']['old_1500ms']}/20 |",
        f"| `aft` (resident watcher) | {fr['aft_resident']['new_0ms']}/20 | {fr['aft_resident']['old_0ms']}/20 | {fr['aft_resident']['new_1500ms']}/20 / {fr['aft_resident']['old_1500ms']}/20 |",
        "",
    ])

    (RESULTS / "COMPETITORS.md").write_text("\n".join(lines), encoding="utf-8")


def main() -> None:
    def ver(cmd: list[str]) -> str:
        return subprocess.run(cmd, capture_output=True, text=True, stdin=subprocess.DEVNULL).stdout.strip().splitlines()[0]

    data: dict = {
        "env": {
            "machine": f"{platform.machine()} {platform.platform()}",
            "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
            "versions": {
                "lancet": ver([LANCET, "--version"]),
                "rg": ver([RG, "--version"]),
                "ast-grep": ver([AST_GREP, "--version"]),
                "tgrep": ver([TGREP, "--version"]),
                "grix": ver([GRIX, "--version"]),
                "aft": ver([AFT, "--version"]),
                "probe": ver([PROBE, "--version"]),
                "tensor-grep": ver([TG, "--version"]),
            },
        },
        "corpora": {},
        "freshness": {},
    }

    ckpt_path = RESULTS / "competitors_agents.json"
    if ckpt_path.exists():
        existing = json.loads(ckpt_path.read_text(encoding="utf-8"))
        for k, v in existing.get("corpora", {}).items():
            data["corpora"][k] = v
            print(f"[resume] Loaded cached corpus results: {k}", flush=True)

    for corpus, lang in TARGET_CORPORA:
        if corpus in data["corpora"]:
            continue
        data["corpora"][corpus] = benchmark_corpus(corpus, lang)
        # Checkpoint incrementally
        ckpt_path.write_text(json.dumps(data, indent=1), encoding="utf-8")

    data["freshness"] = benchmark_freshness()
    (RESULTS / "competitors_agents.json").write_text(json.dumps(data, indent=1), encoding="utf-8")
    write_markdown(data)
    print("\nWrote bench/results/competitors_agents.json and bench/results/COMPETITORS.md")


if __name__ == "__main__":
    main()
