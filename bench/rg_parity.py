#!/usr/bin/env python3
"""Byte-for-byte parity check: `lancet rg` vs ripgrep.

Builds a fixture full of edge cases (Unicode case folding, CRLF, missing final
newline, binary, hidden and ignored files, nested dirs, adjacent words), then
runs both tools with the same flags and compares stdout and exit code.
ripgrep runs with `--sort path` (lancet's output is always path-sorted).

Usage: python3 bench/rg_parity.py [--rg PATH] [--lancet PATH] [--extra DIR]
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

FILES = {
    "a.txt": "foo bar\nFoo Bar\nfoofoo foo,foo\nnothing here\n",
    "a/b.txt": "alpha foo\n\nbeta\nfoo\ngamma\ndelta foo\n",
    "a/deep/c.rs": "fn foo() {}\nlet x = foo(1);\n// foo in comment\nstruct Foo;\n",
    "a/deep/d.md": "# Foo\nfoo bar baz\n",
    "crlf.txt": "foo\r\nbar\r\nfoo bar\r\n",
    "nonl.txt": "last line foo",
    "uni.txt": "Straße STRASSE strasse\nKelvin kelvin KELVIN\n한글 검색 한글\n",
    "sym.txt": "a @x b\n@x\nx@x\n",
    "multi.txt": "foo\nbar\nfoo bar\nbaz\n",
    "empty.txt": "",
    "bin.dat": "foo\0binary\n",
    ".hidden.txt": "foo hidden\n",
    "ignored.log": "foo ignored\n",
    "sub/x.rs": "foo\nfoo\nfoo\nfoo\nfoo\n",
    "sub/y.txt": "one\ntwo foo\nthree\nfour\nfive foo\nsix\nseven\n",
    ".gitignore": "*.log\n",
}

CASES: list[list[str]] = [
    ["foo"],
    ["fo+\\w*"],
    ["-i", "foo"],
    ["-S", "foo"],
    ["-S", "Foo"],
    ["-s", "-i", "foo"],
    ["-i", "-s", "foo"],
    ["-F", "a.b"],
    ["-F", "(1)"],
    ["-w", "foo"],
    ["-w", "@x"],
    ["-x", "foo"],
    ["-v", "foo"],
    ["-c", "foo"],
    ["--count-matches", "foo"],
    ["-l", "foo"],
    ["--files-without-match", "foo"],
    ["-o", "foo"],
    ["-o", "-w", "foo"],
    ["-m", "2", "foo"],
    ["-A", "1", "foo"],
    ["-B", "2", "foo"],
    ["-C", "1", "foo"],
    ["-C", "2", "-m", "1", "foo"],
    ["--column", "foo"],
    ["--vimgrep", "foo"],
    ["-r", "X", "foo"],
    ["-r", "[$1]", "(f)oo"],
    ["-w", "-r", "<$1>", "(f)oo"],
    ["-o", "-r", "$1", "f(o+)"],
    ["-e", "foo", "-e", "bar"],
    ["-g", "*.rs", "foo"],
    ["-g", "!*.md", "foo"],
    ["-g", "!sub/", "foo"],
    ["--iglob", "*.RS", "foo"],
    ["-t", "rust", "foo"],
    ["-T", "rust", "foo"],
    ["-t", "markdown", "-t", "rust", "foo"],
    ["-U", "foo\\nbar"],
    ["--hidden", "foo"],
    ["-u", "foo"],
    ["-uu", "foo"],
    ["-N", "foo"],
    ["-I", "foo"],
    ["-l", "-0", "foo"],
    ["-i", "straße"],
    ["-i", "kelvin"],
    ["한글"],
    ["-q", "foo"],
    ["zzz_no_match"],
    ["-c", "-v", "foo"],
    ["^$"],
    ["-x", ""],
    ["-n", "-A", "1", "-B", "1", "two|five"],
    ["--max-depth", "1", "foo"],
    ["--max-depth", "2", "foo"],
    ["--max-filesize", "20", "foo"],
    ["--max-filesize", "1K", "foo"],
    ["-L", "foo"],
]

FILE_ARG_CASES: list[list[str]] = [
    ["foo", "a.txt"],
    ["-H", "foo", "a.txt"],
    ["foo", "a.txt", "sub"],
    ["foo", "sub/"],
    ["foo", "."],
]

LIST_CASES: list[list[str]] = [
    ["--files"],
    ["--files", "-g", "*.txt"],
    ["--files", "-t", "rust"],
    ["--files", "--hidden"],
]


def run(cmd: list[str], cwd: Path) -> tuple[int, bytes]:
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, stdin=subprocess.DEVNULL)
    return p.returncode, p.stdout


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--rg", default=shutil.which("rg") or "rg")
    ap.add_argument("--lancet", default=str(ROOT / "target" / "release" / "lancet"))
    ap.add_argument("--extra", type=Path, help="also compare a few queries on this directory")
    args = ap.parse_args()

    tmp = Path(tempfile.mkdtemp(prefix="rg-parity-"))
    for rel, body in FILES.items():
        p = tmp / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(body.encode())
    link_target = tmp.parent / f"{tmp.name}-link-target.txt"
    link_target.write_text("foo via symlink\n")
    os.symlink(link_target, tmp / "linked.txt")
    subprocess.run(["git", "init", "-q"], cwd=tmp)  # .gitignore needs a repo

    total = failed = 0

    def check(flags: list[str], cwd: Path, label: str) -> None:
        nonlocal total, failed
        total += 1
        rc_rg, out_rg = run([args.rg, "--sort", "path", "-n", *flags], cwd)
        rc_la, out_la = run([args.lancet, "rg", "-n", *flags], cwd)
        if (rc_rg, out_rg) == (rc_la, out_la):
            return
        failed += 1
        print(f"FAIL {label}: {' '.join(flags)}  (exit rg={rc_rg} lancet={rc_la})")
        rg_lines, la_lines = out_rg.splitlines(), out_la.splitlines()
        for i in range(max(len(rg_lines), len(la_lines))):
            a = rg_lines[i] if i < len(rg_lines) else b"<none>"
            b = la_lines[i] if i < len(la_lines) else b"<none>"
            if a != b:
                print(f"   line {i + 1}: rg     {a!r}\n            lancet {b!r}")
                break

    for c in CASES + FILE_ARG_CASES:
        check(c, tmp, "fixture")
    for c in LIST_CASES:
        # `--files` takes no -n; drop it for both by passing flags directly.
        total += 1
        rc_rg, out_rg = run([args.rg, "--sort", "path", *c], tmp)
        rc_la, out_la = run([args.lancet, "rg", *c], tmp)
        if (rc_rg, out_rg) != (rc_la, out_la):
            failed += 1
            print(f"FAIL fixture: {' '.join(c)}  (exit rg={rc_rg} lancet={rc_la})")
    if args.extra:
        for c in (["fn \\w+_one"], ["-w", "-i", "unwrap"], ["-c", "TODO"], ["-l", "-t", "rust", "Index"]):
            check(c, args.extra.resolve(), str(args.extra))

    shutil.rmtree(tmp)
    link_target.unlink(missing_ok=True)
    print(f"{total - failed}/{total} identical")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
