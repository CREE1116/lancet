---
nav_title: "lancet fd Parity Matrix"
title: "lancet fd — Full fd Parity Matrix"
description: "Every fd 10.5 flag: supported, accepted, or refused with the reason; how output, order and exit codes compare."
lede: "fd's command line and output on lancet's walker, checked against fd 10.5.2 on 133 cases in CI."
keywords: "fd parity glob regex smart case type extension size changed-within owner exclude prune format exec"
source: "https://github.com/CREE1116/lancet/blob/main/docs/fd-parity.md"
---
`lancet fd` takes fd's command line and prints fd's output: the pattern is a regex matched against each entry's name (smart case), `-g` makes it a glob, paths come after the pattern, directories are printed with a trailing `/`, and the exit codes are fd's (`0` whether or not anything matched, `1` for fd's own errors such as a missing search path, `2` for invalid arguments). Underneath it uses lancet's walker, and its index with `--trust-index`.

Every flag of fd 10.5's `--help` is listed below: **supported**, **accepted with no effect**, or **not supported** with the reason.

## Verification {#verification}

- **Fixture parity (`bench/fd_parity.py`, run in CI):** 133 cases over a fixture with hidden and ignored files, `.gitignore` / `.ignore` / `.fdignore` / nested ignore files, symlinks to files and directories and a broken one, an executable, empty files and directories, a FIFO, multi-dot extensions, uppercase, Unicode and space-containing names, a name starting with `-`, exact sizes and an old mtime. Each case compares the exit code and the set of output lines with fd 10.5.2 (fd's output order depends on its threads, so lines are compared sorted; with `--max-results` / `-1` only the count is compared, since which result fd picks is arbitrary).

## Differences {#differences}

1. **Order.** Results are sorted by path, so the output is the same on every run. fd prints them in the order its threads find them.
2. **`--max-results` / `-1`** return the first results in path order, not whichever fd found first.
3. **The index directory** (`.lancet`) is never listed (every lancet command skips it). `.rgignore` files are honoured too, so `fd` and `rg` see the same files.
4. **`-l`, `-x`, `-X`, `--batch-size`** are refused: lancet doesn't run a process per result (the same reason `rg --pre` is refused). Use `lancet fd -0 … | xargs -0 CMD`.
5. **fd's global ignore file** (`~/.config/fd/ignore`) is not read.

---

## Patterns {#patterns}

| Flag | Status | Notes |
|---|---|---|
| `[pattern]` | Supported | A regex matched against the name; `''` or `.` match everything |
| `-g`, `--glob` / `--regex` | Supported | Last one wins |
| `-F`, `--fixed-strings` | Supported | Substring of the name |
| `--exact` | Supported | The whole name (or path with `-p`) |
| `--and PATTERN` | Supported | Repeatable; every pattern must match |
| `-s` / `-i` (`--case-sensitive` / `--ignore-case`) | Supported | Default smart case: sensitive only if a pattern has an uppercase letter (escapes like `\S` don't count) |
| `-p`, `--full-path` | Supported | Matches the absolute path |
| A pattern containing `/` without `-p` | Supported | An error, as in fd; lancet suggests the fd spelling for Glob-tool patterns (`src/**/*.ts` → `lancet fd -g '*.ts' src`) |

## Filters {#filters}

| Flag | Status | Notes |
|---|---|---|
| `-t`, `--type` | Supported | `f`, `d`, `l`, `s`, `p`, `b`, `c`, `x` (implies files), `e` (empty files and directories); repeatable |
| `-e`, `--extension` | Supported | Case-insensitive, leading dot optional, multi-dot (`tar.gz`); matches directories too, as in fd |
| `-S`, `--size` | Supported | `[+-]NUM` with `b`, `k`, `m`, `g`, `t` (powers of 1000) or `ki`, `mi`, `gi`, `ti` (powers of 1024); only files pass |
| `--changed-within` / `--changed-before` and their aliases (`--change-newer-than`, `--newer`, `--changed-after`, `--change-older-than`, `--older`) | Supported | Durations in humantime's syntax (`10h`, `2weeks`, `1h 30min`), `@UNIX_SECONDS`, or a local date `YYYY-MM-DD[ HH:MM:SS]` (dates need Unix) |
| `-o`, `--owner` | Supported (Unix) | `[!]user[:[!]group]`, names or ids |
| `-d`, `--max-depth` / `--min-depth` / `--exact-depth` | Supported | |
| `-E`, `--exclude` | Supported | Glob; excluded directories are not entered |
| `--prune` | Supported | |
| `--ignore-contain NAME` | Supported | |

## Traversal and ignore files {#traversal-and-ignore-files}

| Flag | Status | Notes |
|---|---|---|
| `-H`, `--hidden` / `--no-hidden` | Supported | |
| `-I`, `--no-ignore` / `--ignore` | Supported | |
| `--no-ignore-vcs` / `--ignore-vcs` | Supported | |
| `--no-require-git` / `--require-git` | Supported | |
| `--no-ignore-parent` / `--ignore-parent` | Supported | |
| `-u`, `--unrestricted` | Supported | `--no-ignore --hidden` |
| `--ignore-file PATH` | Supported | |
| `-L`, `--follow` / `--no-follow` | Supported | |
| `--one-file-system` (`--mount`, `--xdev`) | Supported | |
| `-j`, `--threads` | Supported | |

## Paths and output {#paths-and-output}

| Flag | Status | Notes |
|---|---|---|
| `[path]...`, `--search-path` | Supported | Several paths; results are printed as joined onto the path given (`src/`, `./src`, `.` all as in fd) |
| `-C`, `--base-directory` | Supported | |
| `-a`, `--absolute-path` / `--relative-path` | Supported | |
| `-0`, `--print0` | Supported | |
| `--strip-cwd-prefix[=WHEN]` | Supported | fd's rules: `./` is kept with `-0`, and on names starting with `-` |
| `--path-separator` | Supported | |
| `--format` | Supported | `{}`, `{/}`, `{//}`, `{.}`, `{/.}`, `{{`, `}}` |
| `--max-results`, `-1` | Supported | The first results in path order |
| `-q`, `--quiet` (`--has-results`) | Supported | Exit 0 if anything matches, 1 otherwise |
| `-c`, `--color` | Accepted, no effect | lancet prints plain paths |
| `--hyperlink` | Accepted, no effect | |
| `--show-errors` | Accepted, no effect | |
| `-l`, `--list-details` | **Not supported** | Runs `ls`; use `lancet fd -0 … \| xargs -0 ls -l` |
| `-x`, `--exec` / `-X`, `--exec-batch` / `--batch-size` | **Not supported** | lancet doesn't run a process per result; use `xargs -0` |

## lancet additions {#lancet-additions}

| Flag | What it does |
|---|---|
| `--tree` | Draws the results as an indented tree under the search path |
| `--dirs` | Directories only (`-t d`), handy with `--tree` |
| `--type-class CLASS,...` | Files of a class: `code`, `document`, `spreadsheet`, `config`, `log`, `data`, `archive`, `image`, `other` |
| `--json` (`--meta`) | One JSON object with every entry (and size and mtime) |
| `--trust-index` | Lists files from the index without walking the disk; empty directories don't appear |
