#!/usr/bin/env python3
"""Grade agent runs: correctness first, then wall time and round trips; tokens last.

  python3 bench/agents/grade.py WORKDIR/out [-v]

Reads OUT/<task>-<setup>-<tag>.jsonl (OpenCode `--format json` events) and the
matching .diff. Edit tasks are graded from the diff, q2 against the call sites
found in WORKDIR's lancet workspace, q3 from the answer; q1 has no automatic grade.
"""

from __future__ import annotations

import glob
import json
import os
import re
import sys


def stats(f: str) -> dict:
    st = dict(steps=0, calls=0, inp=0, out=0, t0=None, t1=None, answer="", cmds=[], blocked=0)
    for line in open(f):
        try:
            e = json.loads(line)
        except json.JSONDecodeError:
            continue
        ts = e.get("timestamp")
        st["t0"] = st["t0"] or ts
        st["t1"] = ts or st["t1"]
        p = e.get("part", {})
        if e["type"] == "step_finish":
            t = p.get("tokens", {})
            st["steps"] += 1
            st["inp"] += t.get("input", 0) + t.get("cache", {}).get("read", 0)
            st["out"] += t.get("output", 0) + t.get("reasoning", 0)
        elif e["type"] == "tool_use":
            st["calls"] += 1
            i = p["state"].get("input", {})
            c = i.get("command") or i.get("filePath") or i.get("pattern") or json.dumps(i)
            st["cmds"].append(f"{p['tool']}: {c[:100]}".replace("\n", " "))
            if "lancet: command not found" in str(p["state"].get("output", "")):
                st["blocked"] += 1
        elif e["type"] == "text":
            st["answer"] = p.get("text", "")
        elif e["type"] == "error":
            st["answer"] = "ERROR: " + json.dumps(e.get("error"))[:200]
    st["secs"] = round(((st["t1"] or 0) - (st["t0"] or 0)) / 1000)
    return st


def changed_files(diff: str) -> list[str]:
    return sorted(set(re.findall(r"^\+\+\+ b/(\S+)", diff, re.M)))


def q2_truth(work: str) -> set[str]:
    """Calls of `crate::edit::rev` in the workspace (not `.rev()` or a local `rev`)."""
    out = set()
    src = os.path.join(work, "lancet-ws", "src")
    for d, _, fs in os.walk(src):
        for f in fs:
            if f.endswith(".rs"):
                p = os.path.join(d, f)
                for i, line in enumerate(open(p), 1):
                    if re.search(r"(edit::|[^.\w])rev\(", line) and "fn rev(" not in line \
                            and not line.lstrip().startswith("//"):
                        out.add(f"src/{os.path.relpath(p, src)}:{i}")
    return out


def check(task: str, diff: str, answer: str, truth: set[str]):
    fs = changed_files(diff)
    added = [l for l in diff.splitlines() if l.startswith("+") and not l.startswith("+++")]
    removed = [l for l in diff.splitlines() if l.startswith("-") and not l.startswith("---")]
    if task == "e1":
        return fs == ["src/edit_cmd.rs"] and len(removed) == 1 and \
            "no change given (use --lines, --symbol, --old-string or --edits)" in diff
    if task == "e2":
        return len(fs) == 6 and not any("display_path" in l for l in added) and bool(removed)
    if task == "e3":
        return fs == ["src/rg.rs"] and len(added) == 2 and all("search_done" in l for l in added)
    if task == "e4":
        # ReadOptions' field renamed; ReadCli's `budget_tokens` (and the query AST's) kept.
        return set(fs) <= {"src/read_cmd.rs", "src/main.rs"} and "src/read_cmd.rs" in fs \
            and "opts.token_budget = cli.budget_tokens" in diff
    if task == "e5":
        return fs == ["docs/COMMANDS.md"]
    if task == "q2" and truth:
        got = {f"{a}:{b}" for a, b in re.findall(r"(src/[\w/]+\.rs):(\d+)", answer)}
        return truth <= got
    if task == "q3":
        return "48" in answer and re.search(r"\b2\b|two", answer) is not None
    return None


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    if not args:
        sys.exit(__doc__)
    out_dir = args[0]
    work = os.path.dirname(os.path.abspath(out_dir))
    truth = q2_truth(work)
    rows = []
    for f in sorted(glob.glob(os.path.join(out_dir, "*.jsonl"))):
        task, setup, tag = os.path.basename(f)[:-6].split("-")
        s = stats(f)
        dpath = f[:-6] + ".diff"
        diff = open(dpath).read() if os.path.exists(dpath) else ""
        s.update(task=task, setup=setup, tag=tag, ok=check(task, diff, s["answer"], truth))
        rows.append(s)
    if "-v" in sys.argv:
        for r in rows:
            print(f"\n### {r['task']} {r['setup']} {r['tag']} ok={r['ok']} calls={r['calls']}")
            for c in r["cmds"]:
                print("   ", c)
            print("    ANSWER:", r["answer"][:700].replace("\n", "\n     "))
    print(f"{'task':4} {'model':5} | {'setup':8} {'ok':5} {'secs':>5} {'calls':>5} {'steps':>5} {'in_tok':>8} {'out_tok':>7}")
    for r in sorted(rows, key=lambda r: (r["task"], r["tag"], r["setup"])):
        failed = r["answer"].startswith("ERROR")
        print(f"{r['task']:4} {r['tag']:5} | {r['setup']:8} {str(r['ok']):5} {r['secs']:5} {r['calls']:5} "
              f"{r['steps']:5} {r['inp']:8} {r['out']:7}" + ("  (run failed)" if failed else "")
              + (f"  lancet-blocked={r['blocked']}" if r["blocked"] else ""))
    for setup in sorted({r["setup"] for r in rows}):
        rs = [r for r in rows if r["setup"] == setup and not r["answer"].startswith("ERROR")]
        graded = [r for r in rs if r["ok"] is not None]
        print(f"{setup}: correct {sum(1 for r in graded if r['ok'])}/{len(graded)}, "
              f"{sum(r['secs'] for r in rs)} s, {sum(r['calls'] for r in rs)} calls, "
              f"{sum(r['inp'] for r in rs)} input tokens ({len(rs)} runs)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
