---
nav_title: "lancet rg Parity Matrix"
title: "lancet rg — Full ripgrep Parity Matrix"
description: "Exhaustive compatibility table covering matching, formatting, traversal, --passthru, --type-add, and Lancet extensions."
lede: "Complete flag-by-flag comparison verified byte-for-byte against ripgrep 15.2."
keywords: "rg ripgrep parity passthru type-add pcre2 pre docs max-depth max-filesize color"
source: "https://github.com/CREE1116/lancet/blob/main/docs/rg-parity.md"
---
Verified byte-for-byte against `ripgrep 15.2` across 68 edge-case fixtures in `bench/rg_parity.py` (Unicode case folding, CRLF, missing trailing newline, binary skips, `-m` with `-C`, `--vimgrep`, `-L`, `--max-depth`, `--max-filesize`) and Linux v6.12 full-tree queries.

## Patterns & Matching {#matching}

| Flag | Meaning | Status | Notes |
| --- | --- | --- | --- |
| `-e`, `--regexp <PATTERN>` | Search pattern (repeatable) | <span class='tag-yes'>Supported</span> | Combined with regex alternation |
| `-f`, `--file <FILE>` | Read patterns per line (`-` stdin) | <span class='tag-yes'>Supported</span> |  |
| `-F`, `--fixed-strings` | Treat patterns as literals | <span class='tag-yes'>Supported</span> |  |
| `-i` / `-s` / `-S` | Ignore-case / sensitive / smart-case | <span class='tag-yes'>Supported</span> | Last wins; AST-aware `-S` (e.g. `\W` keeps insensitive) |
| `-w`, `--word-regexp` | Surrounded by word boundaries | <span class='tag-yes'>Supported</span> | Matches `rg` boundary behavior (`-w '@x'` works) |
| `-x`, `--line-regexp` | Match spans whole line | <span class='tag-yes'>Supported</span> |  |
| `-v`, `--invert-match` | Lines not matching pattern | <span class='tag-yes'>Supported</span> |  |
| `-U`, `--multiline-dotall` | Multiline match & `.` matches `\n` | <span class='tag-yes'>Supported</span> |  |
| `-P`, `--pcre2` | Lookarounds & backreferences | <span class='tag-no'>Unsupported</span> | Linear-time Rust `regex` + sound HIR trigram lowering |

## Output & Formatting {#output}

| Flag | Meaning | Status | Notes |
| --- | --- | --- | --- |
| `-o`, `-c`, `--count-matches` | Only matches / line count / match count | <span class='tag-yes'>Supported</span> |  |
| `-l`, `--files-without-match`, `-q` | Matching paths / non-matching / quiet | <span class='tag-yes'>Supported</span> |  |
| `-m <N>`, `-A`, `-B`, `-C` | Max matching lines per file & context | <span class='tag-yes'>Supported</span> | Trailing context respects `-m`; emits `--` separators |
| `--passthru` (alias `--passthrough`) | Print matching & non-matching lines | <span class='tag-yes'>Supported</span> | Prints non-matching files too; last of `--passthru/-A/-B/-C` wins |
| `-n` / `-N` | Show / hide line numbers | <span class='tag-yes'>Supported</span> | **ON by default** even when piped |
| `-H` / `-I`, `--column`, `--vimgrep` | Filename visibility, 1-based column, vimgrep | <span class='tag-yes'>Supported</span> | Last of `-H/-I` wins |
| `--json`, `-0`, `-r <TEXT>` | rg JSON Lines, NUL sep, `$1`/`${name}` replace | <span class='tag-yes'>Supported</span> |  |
| `--color <WHEN>` | `auto`, `always`, `ansi`, `never` | <span class='tag-yes'>Supported</span> | `auto` colors TTYs only (`NO_COLOR`/`TERM=dumb` respected) |
| `--sort`, `--no-heading` | Sort / disable grouped headings | No-op | Always path-sorted; never emits headings |

## File Traversal, Filtering & Lancet Extensions {#traversal}

| Flag | Meaning | Status | Notes |
| --- | --- | --- | --- |
| `-g`, `--iglob`, `-t`, `-T`, `--type-list` | Glob & type filters | <span class='tag-yes'>Supported</span> | Positive globs override hidden/ignore like `rg` |
| `--type-add <SPEC>` | `NAME:GLOB` or `NAME:include:A,B` | <span class='tag-yes'>Supported</span> | Applies to `-t`, `-T`, `--files`, `--type-list` |
| `-a`, `-u`/`-uu`/`-uuu`, `-.`, `--no-ignore` | Binary/hidden/ignore overrides | <span class='tag-yes'>Supported</span> |  |
| `-L`, `--max-depth`, `--max-filesize` | Follow symlinks, depth limit, size (`K/M/G`) | <span class='tag-yes'>Supported</span> | `--trust-index -L` falls back to live walk |
| `--files`, `-j/--threads` | Print searched paths, worker count | <span class='tag-yes'>Supported</span> |  |
| `--docs` | Search PDF/Office/HWP/archive text in-process | <span class='tag-ext'>Extension</span> | Off by default for strict `rg` parity |
| `--no-index`, `--trust-index`, `--index-dir` | Control `.lancet` index mode | <span class='tag-ext'>Extension</span> | Default mode is fresh `auto` |
| `--pre <CMD>` | External per-file preprocessor | <span class='tag-no'>Unsupported</span> | Replaced by in-process `--docs` |
