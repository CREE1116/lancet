#!/usr/bin/env python3
"""Reproducible failure modes of tool chains (rg + sed + cat + patch + pdftotext/pandoc
+ rga + zoekt + "write the whole file") versus the same task done with lancet.

Every scenario runs the real tools and records what actually happened, so the
numbers in docs/why-one-tool.md can be regenerated:

  A  stale coordinates: rg -n, then two sed -i edits; the first shifts the second
  B  lost updates: concurrent read-modify-write vs lancet edit --rev
  C  partial patches: patch(1) with one stale hunk vs lancet patch
  D  text refactors hit comments/strings: sed vs AST/refs, on real corpora
  E  document coordinates: rga's line vs the line pandoc/pdftotext show you
  F  tools disagree on which files exist (from the suite run)
  G  stale index: zoekt right after an edit vs lancet auto
  H  context cost: cat a whole file / guess a 50-line window vs read --symbol

Usage: python3 bench/failure_modes.py   (writes bench/results/failure_modes.json)
"""

from __future__ import annotations

import io
import json
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORPORA = ROOT / "bench" / "corpora"
TOOLS = ROOT / "bench" / ".cache" / "tools"
LANCET = str(ROOT / "target" / "release" / "lancet")
RESULTS = ROOT / "bench" / "results"


def tool(name: str) -> str:
    p = TOOLS / name
    return str(p) if p.exists() else (shutil.which(name) or name)


def sh(cmd: list[str], cwd: Path | None = None, stdin: bytes | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=cwd, input=stdin, capture_output=True,
                          stdin=None if stdin is not None else subprocess.DEVNULL)


def lancet_json(args: list[str], cwd: Path | None = None) -> dict:
    p = sh([LANCET, *args, "--json"], cwd=cwd)
    return json.loads(p.stdout or b"{}")


# --------------------------------------------------------------------------- A
def scenario_a() -> dict:
    """An agent finds two call sites with `rg -n`, then edits each with `sed -i`
    using those line numbers. The first replacement adds lines, so the second
    edit lands on the wrong line — silently, exit status 0."""
    d = Path(tempfile.mkdtemp())
    src = "".join(f"def step_{i}():\n    return legacy_call({i})\n\n" for i in range(10))
    f = d / "jobs.py"
    f.write_text(src)
    hits = [int(l.split(b":")[0]) for l in sh(["rg", "-n", "legacy_call", "jobs.py"], cwd=d).stdout.splitlines()]
    first, second = hits[2], hits[6]
    # 1) replace the first call with a 3-line version
    r1 = sh(["sed", "-i", "", f"{first}s/.*/    value = new_call({2})\\n    log(value)\\n    return value/", "jobs.py"], cwd=d)
    # 2) replace the second call using the line number found *before* edit 1
    r2 = sh(["sed", "-i", "", f"{second}s/.*/    return new_call(6)/", "jobs.py"], cwd=d)
    after = f.read_text().splitlines()
    chain_ok = "return new_call(6)" in after and "    return legacy_call(6)" not in after
    clobbered = after[second - 1] if second - 1 < len(after) else None

    # lancet: both hunks in one call, both addressed in the original coordinates
    f.write_text(src)
    hunks = json.dumps([
        {"lines": str(first), "with": "    value = new_call(2)\n    log(value)\n    return value"},
        {"lines": str(second), "with": "    return new_call(6)"},
    ])
    p = sh([LANCET, "edit", "jobs.py", "--edits", "-", "--apply", "--json"], cwd=d, stdin=hunks.encode())
    lan = f.read_text()
    lancet_ok = "return new_call(6)" in lan and "legacy_call(6)" not in lan and "legacy_call(2)" not in lan
    # and sequential edits with --rev: the stale second edit is refused, not misapplied
    f.write_text(src)
    rev = lancet_json(["read", "jobs.py"], cwd=d)["rev"]
    sh([LANCET, "edit", "jobs.py", "--lines", str(first), "--with",
        "    value = new_call(2)\n    log(value)\n    return value", "--rev", rev, "--apply"], cwd=d)
    stale = lancet_json(["edit", "jobs.py", "--lines", str(second), "--with", "    return new_call(6)",
                         "--rev", rev, "--apply"], cwd=d)
    shutil.rmtree(d)
    return {
        "chain": {"sed_exit_codes": [r1.returncode, r2.returncode], "second_edit_correct": chain_ok,
                  "line_overwritten_instead": clobbered},
        "lancet_multi_hunk_correct": lancet_ok and json.loads(p.stdout)["status"] == "complete",
        "lancet_stale_rev_status": stale.get("status"),
    }


# --------------------------------------------------------------------------- B
def scenario_b(trials: int = 200, agents: int = 4) -> dict:
    """`agents` processes each append their own line to the same file at the same
    moment. Chain: read the file, add the line, write the whole file back (what an
    editor tool or `cat > f` does). lancet: read rev, edit --rev, retry on conflict."""
    d = Path(tempfile.mkdtemp())
    f = d / "shared.py"
    lost_chain = 0
    lost_lancet = 0
    conflicts = 0
    lock = threading.Lock()

    def chain_writer(i: int, barrier: threading.Barrier) -> None:
        text = f.read_text()
        barrier.wait()
        f.write_text(text + f"agent_{i}()\n")

    def lancet_writer(i: int, barrier: threading.Barrier, out: list) -> None:
        nonlocal conflicts
        barrier.wait()
        for _ in range(50):  # read -> CAS edit -> on conflict re-read and retry
            rev = lancet_json(["read", str(f)])["rev"]
            n = len(f.read_text().splitlines())
            r = lancet_json(["edit", str(f), "--lines", str(n), "--insert-after", f"agent_{i}()",
                             "--rev", rev, "--apply"])
            if r.get("status") == "complete":
                return
            with lock:
                conflicts += 1
        out.append(i)

    for _ in range(trials):
        f.write_text("base()\n")
        b = threading.Barrier(agents)
        ts = [threading.Thread(target=chain_writer, args=(i, b)) for i in range(agents)]
        [t.start() for t in ts]
        [t.join() for t in ts]
        lost_chain += agents - sum(1 for i in range(agents) if f"agent_{i}()" in f.read_text())
    for _ in range(trials):
        f.write_text("base()\n")
        b = threading.Barrier(agents)
        failed: list = []
        ts = [threading.Thread(target=lancet_writer, args=(i, b, failed)) for i in range(agents)]
        [t.start() for t in ts]
        [t.join() for t in ts]
        lost_lancet += agents - sum(1 for i in range(agents) if f"agent_{i}()" in f.read_text())
    shutil.rmtree(d)
    return {
        "agents": agents,
        "chain": {"trials": trials, "updates": trials * agents, "lost": lost_chain},
        "lancet": {"trials": trials, "updates": trials * agents, "lost": lost_lancet,
                   "conflicts_retried": conflicts},
    }


# --------------------------------------------------------------------------- C
def scenario_c() -> dict:
    """A 2-file diff where file b changed after the diff was made. patch(1) applies
    file a, rejects b, and leaves the repo half-refactored with a .rej file."""
    d = Path(tempfile.mkdtemp())
    (d / "a.py").write_text("def a():\n    return old_name()\n")
    (d / "b.py").write_text("def b():\n    return old_name()\n")
    diff = (
        "--- a/a.py\n+++ b/a.py\n@@ -1,2 +1,2 @@\n def a():\n-    return old_name()\n+    return new_name()\n"
        "--- a/b.py\n+++ b/b.py\n@@ -1,2 +1,2 @@\n def b():\n-    return old_name()\n+    return new_name()\n"
    )
    (d / "b.py").write_text("def b():\n    return old_name(retries=3)\n")  # concurrent change
    snapshot = {p.name: p.read_text() for p in d.glob("*.py")}
    p = sh(["patch", "-p1", "--forward"], cwd=d, stdin=diff.encode())
    chain = {
        "exit": p.returncode,
        "a_changed": (d / "a.py").read_text() != snapshot["a.py"],
        "b_changed": (d / "b.py").read_text() != snapshot["b.py"],
        "rej_files": sorted(x.name for x in d.glob("*.rej")),
    }
    for name, text in snapshot.items():
        (d / name).write_text(text)
    for x in d.glob("*.rej"):
        x.unlink()
    for x in d.glob("*.orig"):
        x.unlink()
    r = sh([LANCET, "patch", "--apply", "--json"], cwd=d, stdin=diff.encode())
    lj = json.loads(r.stdout)
    lancet = {
        "status": lj.get("status"),
        "a_changed": (d / "a.py").read_text() != snapshot["a.py"],
        "b_changed": (d / "b.py").read_text() != snapshot["b.py"],
    }
    shutil.rmtree(d)
    return {"chain_patch1": chain, "lancet": lancet}


# --------------------------------------------------------------------------- D
def scenario_d() -> list[dict]:
    """Renaming an identifier with a text tool (`rg -l | xargs sed -i 's/\\bname\\b/…/'`)
    rewrites every occurrence; `lancet refs` sees only code (definitions + references,
    comments and string literals excluded). The difference is what a text rename
    would also change inside comments and strings — the latter changes behaviour."""
    cases = [("django", "py", "py", "get_queryset"), ("rust", "rs", "rust", "unwrap"),
             ("kubernetes", "go", "go", "Errorf"), ("vscode", "ts", "ts", "dispose"),
             ("spring-framework", "java", "java", "getBean"), ("cpython", "c", "c", "PyErr_SetString")]
    out = []
    for corpus, lang, rgtype, name in cases:
        d = CORPORA / corpus
        if not d.exists():
            continue
        text = len(sh(["rg", "-o", "-w", "-t", rgtype, name, "."], cwd=d).stdout.splitlines())
        refs = lancet_json(["refs", name, ".", "--lang", lang, "--no-index"], cwd=d)
        code = len(refs.get("definitions", [])) + refs.get("total_references", 0)
        out.append({"corpus": corpus, "identifier": name, "text_occurrences": text,
                    "code_occurrences": code, "in_comments_or_strings": text - code,
                    "pct": round(100 * (text - code) / max(text, 1), 1)})
    return out


# --------------------------------------------------------------------------- E
def make_docx(paragraphs: list[str]) -> bytes:
    buf = io.BytesIO()
    body = "".join(f"<w:p><w:r><w:t xml:space=\"preserve\">{p}</w:t></w:r></w:p>" for p in paragraphs)
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("[Content_Types].xml",
                    '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
                    '<Default Extension="xml" ContentType="application/xml"/>'
                    '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
        zf.writestr("_rels/.rels",
                    '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
        zf.writestr("word/document.xml",
                    '<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
                    f"<w:body>{body}</w:body></w:document>")
    return buf.getvalue()


def scenario_e(n_terms: int = 40) -> dict:
    """Search a DOCX with rga (ripgrep-all), then read that line the way an agent
    would (pandoc -t plain | sed -n Np). Wrapped paragraphs shift the numbering, so
    the line you read often isn't the line that matched. lancet search and lancet
    read share one extraction, so the line always contains the term."""
    rnd = random.Random(7)
    words = "alpha beta gamma delta epsilon zeta theta kappa lambda sigma omega quarterly revenue clause".split()
    paras, terms = [], []
    for i in range(120):
        n = rnd.choice([6, 12, 25, 40, 60])
        text = " ".join(rnd.choice(words) for _ in range(n))
        if i % 3 == 0 and len(terms) < n_terms:
            t = f"TERM{i:03d}X"
            text += " " + t
            terms.append(t)
        paras.append(text)
    d = Path(tempfile.mkdtemp())
    (d / "report.docx").write_bytes(make_docx(paras))
    plain = sh(["pandoc", "-t", "plain", "report.docx"], cwd=d).stdout.decode().splitlines()
    rga, lan_ok, chain_ok, rga_found = tool("rga"), 0, 0, 0
    for t in terms:
        r = sh([rga, "-n", "--no-heading", t, "report.docx"], cwd=d).stdout.decode()
        m = re.search(r"(\d+):", r)
        if m:
            rga_found += 1
            ln = int(m.group(1))
            chain_ok += t in (plain[ln - 1] if ln - 1 < len(plain) else "")
        s = lancet_json([t, "report.docx"], cwd=d)
        hit = s["results"][0]["lines"][0]["line"] if s.get("results") else None
        if hit:
            rd = lancet_json(["read", "report.docx", "--lines", str(hit)], cwd=d)
            lan_ok += any(t in l["text"] for l in rd.get("lines", []))
    # formats rga cannot read at all
    sys.path.insert(0, str(ROOT / "bench"))
    from bench_competitors import make_hwpx, make_xlsx  # noqa: E402

    (d / "budget.xlsx").write_bytes(make_xlsx([["item", "H100_CLUSTER_CAPEX"]]))
    (d / "memo.hwpx").write_bytes(make_hwpx("국가클라우드보안인증검토"))
    unsupported = {}
    for fname, t in (("budget.xlsx", "H100_CLUSTER_CAPEX"), ("memo.hwpx", "국가클라우드보안인증검토")):
        unsupported[fname] = {
            "rga_hits": len(sh([rga, "-l", t, fname], cwd=d).stdout.splitlines()),
            "lancet_hits": lancet_json([t, fname], cwd=d).get("matched", 0),
        }
    shutil.rmtree(d)
    return {"terms": len(terms), "rga_found": rga_found,
            "chain_line_contains_term": chain_ok, "lancet_line_contains_term": lan_ok,
            "unsupported_by_rga": unsupported}


# --------------------------------------------------------------------------- G
def scenario_g() -> dict:
    """Index a copy of django with zoekt and lancet, then edit 20 files (remove an
    OLD token, add a FRESH one) and query immediately, as an agent would."""
    src = CORPORA / "django"
    if not src.exists():
        return {}
    d = Path(tempfile.mkdtemp()) / "django"
    shutil.copytree(src, d, ignore=shutil.ignore_patterns(".git"))
    files = sorted(d.rglob("*.py"))[:400:20]  # 20 files
    for i, f in enumerate(files):
        f.write_text(f.read_text() + f"\nOLD_TOKEN_{i:02d} = {i}\n")
    zidx = d.parent / "zoekt"
    zidx.mkdir()
    sh([tool("zoekt-index"), "-index", str(zidx), "-disable_ctags", str(d)])
    sh([LANCET, "index", str(d)])
    for i, f in enumerate(files):
        f.write_text(f.read_text().replace(f"OLD_TOKEN_{i:02d}", f"FRESH_TOKEN_{i:02d}"))
    zl = lambda q: len(sh([tool("zoekt"), "-index_dir", str(zidx), "-l", q]).stdout.splitlines())  # noqa: E731
    ll = lambda q: len(lancet_json([f'"{q}" | files', str(d)]).get("results", []))  # noqa: E731
    res = {"edited_files": len(files),
           "zoekt": {"finds_new_token_in": zl("FRESH_TOKEN_"), "still_finds_removed_token_in": zl("OLD_TOKEN_")},
           "lancet_auto": {"finds_new_token_in": ll("FRESH_TOKEN_"), "still_finds_removed_token_in": ll("OLD_TOKEN_")}}
    shutil.rmtree(d.parent)
    return res


# --------------------------------------------------------------------------- H
def scenario_h(per_corpus: int = 60) -> list[dict]:
    """Reading one function: bytes of `cat file`, whether a 50-line window from
    the definition line (`sed -n 'L,L+49p'`) covers the whole function, and bytes
    of `lancet read --symbol`."""
    plan = [("django", "py", r"^\s*def (\w+)\("), ("rust", "rs", r"^\s*(?:pub )?fn (\w+)"),
            ("kubernetes", "go", r"^func (?:\([^)]*\) )?(\w+)\("), ("vscode", "ts", r"^\s*(?:export )?function (\w+)"),
            ("spring-framework", "java", r"^\s*public \w[\w<>, ]* (\w+)\("), ("linux", "c", r"^\w[\w\s\*]* (\w+)\([^;]*$")]
    rnd = random.Random(11)
    out = []
    for corpus, ext, rx in plan:
        d = CORPORA / corpus
        if not d.exists():
            continue
        files = [l.decode() for l in sh(["rg", "--files", "-g", f"*.{ext}"], cwd=d).stdout.splitlines()]
        rnd.shuffle(files)
        rec = {"corpus": corpus, "functions": 0, "cat_bytes": [], "symbol_bytes": [], "window_truncated": 0}
        pat = re.compile(rx)
        for rel in files:
            if rec["functions"] >= per_corpus:
                break
            try:
                text = (d / rel).read_text()
            except (UnicodeDecodeError, OSError):
                continue
            names = [(i, m.group(1)) for i, l in enumerate(text.splitlines()) if (m := pat.match(l))]
            if not names:
                continue
            _, name = rnd.choice(names)
            r = lancet_json(["read", rel, "--symbol", name], cwd=d)
            if r.get("status") != "complete":
                continue
            body = "\n".join(l["text"] for l in r["lines"])
            rec["functions"] += 1
            rec["cat_bytes"].append(len(text.encode()))
            rec["symbol_bytes"].append(len(body.encode()))
            rec["window_truncated"] += (r["end_line"] - r["start_line"] + 1) > 50
        n = max(rec["functions"], 1)
        med = lambda v: sorted(v)[len(v) // 2] if v else 0  # noqa: E731
        rec.update({
            "median_cat_bytes": med(rec.pop("cat_bytes")),
            "median_symbol_bytes": med(rec.pop("symbol_bytes")),
            "window_truncated_pct": round(100 * rec["window_truncated"] / n, 1),
        })
        out.append(rec)
    return out


# --------------------------------------------------------------------------- I
def scenario_i() -> dict:
    """Does an edit keep the file the same file? Edit through a symlink, edit a
    file with two hard links, edit a 0640 file — with the usual tools."""
    def atomic_py(path: Path, new: str) -> None:  # the classic editor-tool write
        tmp = path.with_name(path.name + ".tmp")
        tmp.write_text(new)
        os.replace(tmp, path)

    tools = {
        "perl -pi": lambda p: sh(["perl", "-pi", "-e", "s/old/new/", str(p)]),
        "sed -i": lambda p: sh(["sed", "-i", "", "s/old/new/", str(p)]),
        "tmp + rename (editor tools)": lambda p: atomic_py(p, p.read_text().replace("old", "new")),
        "lancet edit": lambda p: sh([LANCET, "edit", str(p), "--lines", "1", "--with", "new", "--apply"]),
    }
    out = {}
    for name, edit in tools.items():
        d = Path(tempfile.mkdtemp())
        (d / "real.txt").write_text("old\n")
        (d / "link.txt").symlink_to("real.txt")
        (d / "h1.txt").write_text("old\n")
        os.link(d / "h1.txt", d / "h2.txt")
        (d / "mode.txt").write_text("old\n")
        os.chmod(d / "mode.txt", 0o640)
        edit(d / "link.txt")
        edit(d / "h1.txt")
        edit(d / "mode.txt")
        out[name] = {
            "symlink_kept_and_target_edited": (d / "link.txt").is_symlink() and (d / "real.txt").read_text() == "new\n",
            "hard_link_still_shared": (d / "h2.txt").read_text() == (d / "h1.txt").read_text() == "new\n",
            "mode_kept": oct(os.stat(d / "mode.txt").st_mode & 0o777) == "0o640",
        }
        shutil.rmtree(d)
    return out


def main() -> int:
    RESULTS.mkdir(parents=True, exist_ok=True)
    res = {}
    for key, fn in (("A_stale_coordinates", scenario_a), ("B_lost_updates", scenario_b),
                    ("C_partial_patch", scenario_c), ("D_text_refactor_false_hits", scenario_d),
                    ("E_document_coordinates", scenario_e), ("G_stale_index", scenario_g),
                    ("H_context_cost", scenario_h), ("I_file_identity", scenario_i)):
        print(f"== {key}", flush=True)
        try:
            res[key] = fn()
        except Exception as e:  # keep going; record the failure
            res[key] = {"error": repr(e)}
        print(json.dumps(res[key], ensure_ascii=False, indent=1)[:1500], flush=True)
    (RESULTS / "failure_modes.json").write_text(json.dumps(res, ensure_ascii=False, indent=1))
    print("wrote bench/results/failure_modes.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
