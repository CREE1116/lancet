#!/usr/bin/env python3
"""Parity check: `lancet fd` vs fd.

Builds a fixture full of edge cases (hidden and ignored files, .fdignore,
symlinks, an executable, empty files and directories, multi-dot extensions,
uppercase and Unicode names, sizes, old mtimes, a FIFO), runs both tools with
the same arguments and compares the set of output lines and the exit code.
fd prints results in whatever order its threads find them, so lines are
compared sorted (lancet's output is always path-sorted). With --max-results /
-1 only the number of lines is compared, since fd's choice of result is
arbitrary.

Usage: python3 bench/fd_parity.py [--fd PATH] [--lancet PATH]
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
    "src/main.rs": "fn main() {}\n",
    "src/Lib.RS": "x",
    "src/sub/empty.txt": "",
    "src/sub/deep/inner.py": "print(1)\n",
    "src/sub/deep/deeper/leaf.go": "package x\n",
    "docs/README.md": "# readme\n",
    "docs/guide.MD": "guide\n",
    "docs/한글 문서.txt": "unicode\n",
    "docs/with space.txt": "space\n",
    "archive.tar.gz": "x" * 10,
    "dir.rs/inside.txt": "a dir whose name ends in .rs\n",
    ".hidden/h.txt": "hidden\n",
    ".hidden.cfg": "hidden file\n",
    "build/out.o": "ignored by .gitignore\n",
    "logs/app.log": "ignored by pattern\n",
    "fdign/skip.txt": "ignored by .fdignore\n",
    "dotign.txt": "ignored by .ignore\n",
    "nested/.gitignore": "local.tmp\n",
    "nested/local.tmp": "ignored by nested gitignore\n",
    "nested/keep.tmp": "kept\n",
    "sizes/k1000.bin": "a" * 1000,
    "sizes/k1024.bin": "a" * 1024,
    "sizes/k2000.bin": "a" * 2000,
    "old/ancient.txt": "old\n",
    "proj/marker.cfg": "",
    "proj/code.c": "int x;\n",
    "-dash.txt": "starts with a dash\n",
    "UPPER.TXT": "upper\n",
    "run.sh": "#!/bin/sh\n",
    ".gitignore": "build/\n*.log\n",
    ".fdignore": "fdign/\n",
    ".ignore": "dotign.txt\n",
    "custom.ignore": "docs/guide.MD\n",
}


def build(d: Path) -> None:
    subprocess.run(["git", "init", "-q", str(d)], check=True)
    for rel, body in FILES.items():
        p = d / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(body, encoding="utf-8")
    (d / "emptydir").mkdir()
    (d / "src/sub/emptyinner").mkdir()
    os.chmod(d / "run.sh", 0o755)
    os.symlink("src/main.rs", d / "link.rs")
    os.symlink("src", d / "linkdir")
    os.symlink("nowhere", d / "broken.lnk")
    os.mkfifo(d / "pipe.fifo")
    old = 978307200  # 2001-01-01
    os.utime(d / "old/ancient.txt", (old, old))


# Each case: arguments after the binary name.
CASES = [
    [], ["main"], ["Main"], ["MAIN"], ["rs"], [r"\.rs$"], ["^m"], ["a.n"], ["."], [""],
    ["-s", "lib"], ["-i", "LIB"], ["-s", "Lib"], ["[A-Z]"], [r"\S+\.md"],
    ["-g", "*.rs"], ["-g", "*.RS"], ["-g", "-s", "*.rs"], ["-g", "main.*"], ["-g", "{main,Lib}.*"],
    ["-F", "."], ["-F", "a.n"], ["--exact", "main.rs"], ["--exact", "main"], ["-F", "-i", "upper"],
    ["--and", "s", "ma"], ["--and", "x", "--and", "t", "."],
    ["-p", "src.*main"], ["-p", "-g", "**/src/*.rs"], ["-p", "deep"],
    ["-e", "rs"], ["-e", ".RS"], ["-e", "md"], ["-e", "gz"], ["-e", "tar.gz"], ["-e", "rs", "-e", "md"],
    ["-e", "txt", "docs"], ["-e", "rs", "-t", "f"],
    ["-t", "f"], ["-t", "d"], ["-t", "l"], ["-t", "x"], ["-t", "e"], ["-t", "p"],
    ["-te", "-tf"], ["-te", "-td"], ["-t", "x", "-t", "d"], ["-t", "f", "-t", "l"], ["-t", "directory"],
    ["-H"], ["-H", "h"], ["-H", "-t", "f", "cfg"], ["-I"], ["-I", "log"], ["-u"], ["-u", "-t", "f", "skip"],
    ["--no-ignore-vcs"], ["--no-ignore-vcs", "out"], ["--ignore-file", "custom.ignore"],
    ["-d", "1"], ["-d", "2", "-t", "f"], ["--min-depth", "3"], ["--exact-depth", "2"], ["--max-depth", "0"],
    ["-E", "src"], ["-E", "*.txt"], ["-E", "docs", "-E", "src", "-t", "f"], ["-E", "deep"],
    ["--prune", "src"], ["--prune", "sub"],
    ["-S", "+1k"], ["-S", "-1k"], ["-S", "+1ki"], ["-S", "1000b"], ["-S", "+1k", "-S", "-2k"], ["-S", "+0b", "-t", "d"],
    ["--changed-within", "1d"], ["--changed-before", "1d"], ["--changed-before", "2005-01-01"],
    ["--changed-within", "2005-01-01 00:00:00"], ["--newer", "@0"], ["--older", "10years"], ["--changed-within", "1h 30min"],
    ["-o", "!root"],
    ["main", "src"], ["main", "src/"], ["main", "./src"], ["main", "."], ["main", "src", "docs"],
    ["-t", "f", ".", "src/sub"], ["--search-path", "src", "main"], ["--search-path", "src", "--search-path", "docs"],
    ["-a", "main"], ["-a", "-t", "d", "sub"],
    ["-0", "main"], ["-0", "-t", "d"], ["-0", "main", "src"], ["--strip-cwd-prefix=never", "main"],
    ["--strip-cwd-prefix", "-0", "main"], ["--strip-cwd-prefix=always", "main"],
    ["--format", "{} {/} {//} {.} {/.}", "main"], ["--format", "{}", "-t", "d"], ["--format", "{//}", "run"],
    ["--format", "[{/.}]", "-e", "gz"], ["--format", "{{}}", "main"],
    ["--path-separator", "+", "main"], ["--path-separator", "+", "-t", "d", "deep"],
    ["-C", "src", "main"], ["-C", "src", "-t", "d"],
    ["-L", "-t", "f", "main"], ["-L", "main"], ["-L", "-t", "d"],
    ["--ignore-contain", "marker.cfg"], ["--ignore-contain", "marker.cfg", "-t", "f"],
    ["-q", "main"], ["-q", "zzz"], ["zzz"], ["--has-results", "run"],
    ["-1"], ["--max-results", "3"], ["--max-results", "100"],
    ["--", "-dash"], ["-F", "--", "-dash"],
    ["한글"], ["space"],
    # Errors: exit codes must match.
    ["src/sub"], ["a/b"], ["main", "nothere"], ["main", "run.sh"], ["-t", "z"], ["-S", "10"], ["("],
    ["--changed-within", "yesterday"],
]

LIMITED = {"-1", "--max-results"}


def run(cmd: list[str], cwd: Path) -> tuple[int, list[bytes]]:
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, timeout=60)
    sep = b"\0" if "-0" in cmd else b"\n"
    lines = [x for x in p.stdout.split(sep) if x]
    return p.returncode, sorted(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--fd", default=shutil.which("fd") or str(ROOT / "bench/.cache/tools/fd"))
    ap.add_argument("--lancet", default=str(ROOT / "target/release/lancet"))
    ap.add_argument("-v", action="store_true", help="show both outputs for every mismatch")
    args = ap.parse_args()
    ver = subprocess.run([args.fd, "--version"], capture_output=True, text=True).stdout.strip()
    with tempfile.TemporaryDirectory() as td:
        d = Path(td).resolve() / "fx"
        d.mkdir()
        build(d)
        bad = 0
        for c in CASES:
            fe, fo = run([args.fd, *c], d)
            le, lo = run([args.lancet, "fd", *c], d)
            limited = any(x in LIMITED for x in c)
            same = fe == le and (len(fo) == len(lo) if limited else fo == lo)
            if not same:
                bad += 1
                print(f"DIFF fd {' '.join(c)}: exit fd={fe} lancet={le}")
                if args.v or True:
                    only_fd = [x for x in fo if x not in lo][:6]
                    only_l = [x for x in lo if x not in fo][:6]
                    print(f"   only fd:     {only_fd}")
                    print(f"   only lancet: {only_l}")
        print(f"{len(CASES) - bad}/{len(CASES)} identical ({ver})")
        return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
