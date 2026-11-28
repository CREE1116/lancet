#!/usr/bin/env python3
"""Rename oracle: does `lancet rename` keep the program's meaning?

Samples local bindings in a real codebase, renames each one to a fresh name with
`lancet rename --apply` (on a copy; the corpus is never touched), checks the result
with the language's own tools, then restores the file before the next rename.

  Rust    the crate still builds with no new errors or warnings (`cargo check`)
  Python  every scope's symbol table is the same apart from the new name
          (`symtable`; needs Python <= 3.11, since 3.12 inlines comprehensions)
  Go      the package still builds and vets (`go build`, `go vet`)

A refusal is fine (lancet says why and writes nothing); a rename that changes the
program is a bug. Exit status 1 if any rename was wrong.

  python3 bench/rename_oracle.py rust .                       # lancet's own source
  /usr/bin/python3 bench/rename_oracle.py python bench/corpora/django --limit 300
  python3 bench/rename_oracle.py go bench/corpora/kubernetes --only pkg/util --limit 100
"""

from __future__ import annotations

import argparse
import collections
import json
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LANCET = os.environ.get("LANCET", str(ROOT / "target" / "release" / "lancet"))

# A binding at the start of a statement; group 1 is the name.
PATTERNS = {
    "rust": (".rs", re.compile(r"^\s*let (?:mut )?([a-z_][a-z0-9_]*)\s*[:=]")),
    "python": (".py", re.compile(r"^\s{8,}([a-z_][a-z0-9_]*)\s*=[^=](?!.*,\s*$)")),  # inside a function body
    "go": (".go", re.compile(r"^\s+([a-z][A-Za-z0-9]*)(?:\s*,\s*[a-z_][A-Za-z0-9_]*)*\s*:=")),
}
SKIP_DIRS = {".git", "target", "node_modules", "vendor", "testdata", ".lancet", "__pycache__",
             "corpora", ".cache"}


def candidates(root: Path, ext: str, pat: re.Pattern) -> list[tuple[Path, int, int, str]]:
    out = []
    for d, dirs, files in os.walk(root):
        dirs[:] = sorted(x for x in dirs if x not in SKIP_DIRS)
        for f in sorted(files):
            if not f.endswith(ext):
                continue
            p = Path(d) / f
            try:
                lines = p.read_text().split("\n")
            except (UnicodeDecodeError, OSError):
                continue
            for i, line in enumerate(lines, 1):
                m = pat.match(line)
                if m and m.group(1) != "_":
                    out.append((p, i, m.start(1) + 1, m.group(1)))
    return out


def fresh(name: str, n: int) -> str:
    # Keep a leading underscore: it tells compilers the binding may be unused.
    lead = "_" if name.startswith("_") else ""
    return f"{lead}zq{n}_{name.lstrip('_')}"


# ----------------------------------------------------------------- checkers
class Rust:
    def __init__(self, work: Path):
        self.work = work
        self.env = dict(os.environ, CARGO_TARGET_DIR=str(work.parent / "target"))
        self.baseline = self._diagnostics()
        if self.baseline is None:
            sys.exit("the crate doesn't build before any rename")

    def _diagnostics(self) -> set[str] | None:
        p = subprocess.run(["cargo", "check", "--all-targets", "--message-format=short"],
                           cwd=self.work, env=self.env, capture_output=True, text=True)
        if p.returncode != 0:
            return None
        return {re.sub(r":\d+:\d+", "", l) for l in p.stderr.splitlines() if ": warning:" in l}

    def ok(self, path: Path, before: str) -> tuple[bool, str]:
        d = self._diagnostics()
        if d is None:
            return False, "no longer builds"
        new = d - self.baseline
        return (not new), "; ".join(sorted(new))[:300]


class Python:
    def __init__(self, work: Path):
        if sys.version_info >= (3, 12):
            sys.exit("run the Python oracle with Python <= 3.11 (3.12+ inlines comprehensions "
                     "into their function's symbol table), e.g. /usr/bin/python3 on macOS")
        import symtable
        self.symtable = symtable

    def table(self, src: str, fn: str):
        out = []

        def walk(t, path):
            syms = sorted((re.sub(r"^_?zq\d+_", "", s.get_name()), s.is_local(), s.is_global(),
                           s.is_free(), s.is_parameter(), s.is_referenced(), s.is_assigned(),
                           s.is_imported()) for s in t.get_symbols())
            out.append((path, str(t.get_type()), syms))
            for i, c in enumerate(t.get_children()):
                walk(c, path + (re.sub(r"^_?zq\d+_", "", c.get_name()), i))

        walk(self.symtable.symtable(src, fn, "exec"), ())
        return out

    def ok(self, path: Path, before: str) -> tuple[bool, str]:
        after = path.read_text()
        try:
            compile(after, str(path), "exec")
            return self.table(before, str(path)) == self.table(after, str(path)), "symbol tables differ"
        except (SyntaxError, ValueError) as e:
            return False, f"no longer parses: {e}"

    def builds(self, path: Path) -> bool:
        try:
            compile(path.read_text(), str(path), "exec")
            return True
        except (SyntaxError, ValueError):
            return False


class Go:
    def __init__(self, work: Path):
        self.work = work

    def ok(self, path: Path, before: str) -> tuple[bool, str]:
        pkg = path.parent
        for cmd in (["go", "build", "."], ["go", "vet", "."]):
            p = subprocess.run(cmd, cwd=pkg, capture_output=True, text=True)
            if p.returncode != 0:
                return False, f"{' '.join(cmd)}: {p.stderr.strip()[:300]}"
        return True, ""

    def builds(self, path: Path) -> bool:
        return self.ok(path, "")[0]


CHECKERS = {"rust": Rust, "python": Python, "go": Go}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("lang", choices=sorted(PATTERNS))
    ap.add_argument("corpus", type=Path)
    ap.add_argument("--limit", type=int, default=150, help="renames to try (default 150)")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--only", default="", help="rename only under this subdirectory of CORPUS")
    args = ap.parse_args()

    ext, pat = PATTERNS[args.lang]
    tmp = Path(tempfile.mkdtemp(prefix="lancet-oracle-"))
    work = tmp / "src"
    shutil.copytree(args.corpus, work, ignore=shutil.ignore_patterns(*SKIP_DIRS - {"vendor", "testdata"}))
    try:
        checker = CHECKERS[args.lang](work)
        cands = candidates(work / args.only, ext, pat)
        random.Random(args.seed).shuffle(cands)
        stats: collections.Counter[str] = collections.Counter()
        bad = []
        tried = 0
        for path, line, col, name in cands:
            if tried >= args.limit:
                break
            before = path.read_text()
            if hasattr(checker, "builds") and not checker.builds(path):
                # A file this Python can't parse (newer syntax), or a Go package that
                # doesn't build on its own, can't judge a rename.
                continue
            tried += 1
            new = fresh(name, tried)
            r = subprocess.run([LANCET, "rename", f"{path}:{line}:{col}", name, new, "--apply", "--json"],
                               capture_output=True, text=True)
            try:
                d = json.loads(r.stdout)
            except json.JSONDecodeError:
                stats["CRASH"] += 1
                bad.append((path, line, name, r.stderr.strip()[:300]))
                continue
            if d["status"] != "complete":
                reason = re.sub(r"`[^`]*`|\d+:\d+", "_", (d.get("error") or {}).get("message", "?"))
                stats["refused: " + reason[:70]] += 1
                continue
            good, why = checker.ok(path, before)
            stats["renamed" if good else "WRONG"] += 1
            if not good:
                diff = subprocess.run(["diff", "-u", "-", str(path)], input=before,
                                      capture_output=True, text=True).stdout
                bad.append((path, line, name, why + "\n" + diff[:1500]))
            path.write_text(before)
        print(f"{args.lang}: {tried} renames tried in {args.corpus}")
        for k, v in stats.most_common():
            print(f"  {v:5} {k}")
        for path, line, name, why in bad:
            print(f"\nWRONG {path.relative_to(work)}:{line} `{name}`: {why}")
        return 1 if bad else 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
