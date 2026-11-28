#!/usr/bin/env python3
"""Compare `lancet ast` with ast-grep on real code: which matches each finds.

For every (language, corpus, pattern) case, both tools search the same tree
(no index) and their matches are compared by (file, start byte, end byte).
The report counts matches found by both, only by ast-grep and only by lancet,
and prints a few examples of each difference, so docs/ast-grep-parity.md can
state measured differences instead of guesses.

Usage: python3 bench/ast_parity.py [--sg PATH] [--lancet PATH] [--examples N]
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
C = ROOT / "bench" / "corpora"

CASES = [
    ("rs", "rust/compiler/rustc_middle", "$X.unwrap()"),
    ("rs", "rust/compiler/rustc_middle", "$X.expect($M)"),
    ("rs", "rust/compiler/rustc_middle", "panic!($$$A)"),
    ("rs", "rust/compiler/rustc_middle", "Some($X)"),
    ("rs", "rust/compiler/rustc_middle", "let $X = $Y;"),
    ("rs", "rust/compiler/rustc_middle", "if $C { $$$B }"),
    ("rs", "rust/compiler/rustc_middle", "$A == $A"),
    ("rs", "rust/compiler/rustc_middle", "unsafe { $$$B }"),
    ("rs", "rust/compiler/rustc_middle", "match $X { $$$A }"),
    ("rs", "rust/compiler/rustc_middle", "fn $F($$$P) -> $R { $$$B }"),
    ("py", "django/django/db", "print($$$A)"),
    ("py", "django/django/db", "if $C: return $X"),
    ("py", "django/django/db", "$X is None"),
    ("py", "django/django/db", "raise $E($$$A)"),
    ("py", "django/django/db", "isinstance($X, $T)"),
    ("py", "django/django/db", "self.$F($$$A)"),
    ("py", "django/django/db", "def $F(self): $$$B"),
    ("py", "django/django/db", "return $X"),
    ("go", "kubernetes/pkg/util", "fmt.Errorf($$$A)"),
    ("go", "kubernetes/pkg/util", "if err != nil { return $$$R }"),
    ("go", "kubernetes/pkg/util", "$X := $Y"),
    ("go", "kubernetes/pkg/util", "errors.New($S)"),
    ("go", "kubernetes/pkg/util", "for $I := range $X { $$$B }"),
    ("ts", "vscode/src/vs/base", "console.log($$$A)"),
    ("ts", "vscode/src/vs/base", "new Error($M)"),
    ("ts", "vscode/src/vs/base", "if ($C) { $$$B }"),
    ("ts", "vscode/src/vs/base", "$X === undefined"),
    ("ts", "vscode/src/vs/base", "await $X"),
    ("c", "linux/kernel/sched", "spin_lock($X)"),
    ("c", "linux/kernel/sched", "if ($C) return $X;"),
    ("c", "linux/kernel/sched", "$X->$Y = $Z"),
    ("java", "spring-framework/spring-core/src/main", "Assert.notNull($X, $M)"),
    ("java", "spring-framework/spring-core/src/main", "if ($C) { $$$B }"),
    ("java", "spring-framework/spring-core/src/main", "throw new $E($$$A);"),
]


def sg_matches(sg: str, lang: str, root: Path, pat: str) -> set[tuple[str, int, int]]:
    p = subprocess.run([sg, "run", "-p", pat, "-l", lang, "--json=compact", str(root)],
                       capture_output=True, text=True, timeout=600)
    try:
        items = json.loads(p.stdout or "[]")
    except json.JSONDecodeError:
        return set()
    return {(str(Path(m["file"]).resolve()), m["range"]["byteOffset"]["start"], m["range"]["byteOffset"]["end"])
            for m in items}


def lancet_matches(lancet: str, lang: str, root: Path, pat: str) -> tuple[set[tuple[str, int, int]], str]:
    p = subprocess.run([lancet, "ast", pat, str(root), "--lang", lang, "--no-index", "--json"],
                       capture_output=True, text=True, timeout=600)
    try:
        d = json.loads(p.stdout)
    except json.JSONDecodeError:
        return set(), p.stderr.strip()[:200]
    err = (d.get("error") or {}).get("message", "")
    return {(str(Path(m["path"]).resolve()), m["start_byte"], m["end_byte"]) for m in d.get("matches", [])}, err


def snippet(key: tuple[str, int, int]) -> str:
    f, s, e = key
    try:
        b = Path(f).read_bytes()
    except OSError:
        return "?"
    line = b[:s].count(b"\n") + 1
    text = b[s:e].decode("utf-8", "replace").replace("\n", "⏎")
    return f"{Path(f).relative_to(C)}:{line}: {text[:90]}"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--sg", default=str(ROOT / "bench/.cache/tools/ast-grep"))
    ap.add_argument("--lancet", default=str(ROOT / "target/release/lancet"))
    ap.add_argument("--examples", type=int, default=2)
    args = ap.parse_args()
    ver = subprocess.run([args.sg, "--version"], capture_output=True, text=True).stdout.strip()
    tot_both = tot_sg = tot_l = 0
    print(f"| Lang | Pattern | both | only ast-grep | only lancet |\n|---|---|---:|---:|---:|")
    notes = []
    for lang, sub, pat in CASES:
        root = C / sub
        if not root.exists():
            continue
        a = sg_matches(args.sg, lang, root, pat)
        b, err = lancet_matches(args.lancet, lang, root, pat)
        both, only_a, only_b = a & b, a - b, b - a
        tot_both += len(both); tot_sg += len(only_a); tot_l += len(only_b)
        flag = f" (lancet: {err})" if err else ""
        print(f"| {lang} | `{pat}` | {len(both)} | {len(only_a)} | {len(only_b)}{flag} |", flush=True)
        for k in sorted(only_a)[: args.examples]:
            notes.append(f"  only ast-grep  [{lang}] {pat!r}: {snippet(k)}")
        for k in sorted(only_b)[: args.examples]:
            notes.append(f"  only lancet    [{lang}] {pat!r}: {snippet(k)}")
    total = tot_both + tot_sg + tot_l
    print(f"\nTotal: {tot_both} matches found by both, {tot_sg} only by ast-grep, {tot_l} only by lancet "
          f"({100 * tot_both / max(total, 1):.1f}% agreement; {ver}).")
    if notes:
        print("\nExamples:")
        print("\n".join(notes))
    return 0


if __name__ == "__main__":
    sys.exit(main())
