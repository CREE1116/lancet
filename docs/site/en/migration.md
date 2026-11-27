---
nav_title: "Migration from rg, fd, sg"
title: "Coming from rg, fd, ast-grep, sed, patch & rga"
description: "Side-by-side Rosetta Stone, intentional differences, and when to keep git or an LSP."
lede: "Keep familiar flags from ripgrep, fd, and ast-grep while eliminating line drift and silent overwrites."
keywords: "migration ripgrep rg fd ast-grep sg sed patch rga pcre2 lsp differences"
source: "https://github.com/CREE1116/lancet/blob/main/docs/rg-parity.md"
---
You can switch incrementally without rewriting muscle memory. Use `lancet rg` and `lancet patch` as drop-in replacements for existing flags and diff pipes, then adopt LQL, AST rewrites, and `--rev` CAS edits where multi-tool seams break.

## Coming from ripgrep (rg) {#from-rg}

`lancet rg` implements ripgrep's flags, Rust `regex` engine, ignore traversal, exit codes (`0` match, `1` no match, `2` error), and output formats — tested byte-for-byte against `ripgrep 15.2` across 68 edge-case fixtures and full Linux kernel runs.

| ripgrep Command | lancet Equivalent | Exact Behavioral Note |
| --- | --- | --- |
| `rg foo src` | `lancet rg foo src` | `-n` is ON by default even when piped; pass `-N` to hide |
| `rg -i -w -t py tok` | `lancet rg -i -w -t py tok` | Last of `-i`/`-s`/`-S` wins; AST-aware smart case (`\W` keeps insensitive) |
| `rg -g '*.toml' --passthru` | `lancet rg -g '*.toml' --passthru` | Last of `--passthru`/`-A`/`-B`/`-C` wins; prints unmatched files too |
| `rg --type-add 'web:*.tsx'` | `lancet rg --type-add 'web:*.tsx' -t web` | Supports `name:glob` and `name:include:a,b` |
| `rg -P '(?<=a)b'` | `lancet 'before("a", "b", 1)'` or Rust regex | `-P`/PCRE2 omitted for linear-time matching & sound trigram lowering |
| `rg --pre pdftotext pat` | `lancet rg --docs pat` | In-process PDF/Office/HWP/archive extraction instead of subprocesses |

> [!NOTE] Intentional differences in lancet rg
> - **Always sorted by path component:** `a/b.txt` before `a.txt`, equivalent to `rg --sort path`.
> - **Line numbers ON by default:** stays enabled under pipes/redirects unless `-N` is passed.
> - **No headings:** output is always `path:line:text`, with `--color auto` on TTYs only.

## Coming from fd / find {#from-fd}

| fd / find Command | lancet fd Equivalent | Note / Advantage |
| --- | --- | --- |
| `fd -e rs -d 2 src` | `lancet fd -e rs -d 2 src docs` | Uses `.lancet` when present; accepts `[PATTERN] [PATH...]` |
| `fd -t d -d 2` | `lancet fd --tree --dirs -d 2 .` | Folders-only ASCII tree for compact orientation |
| `fd '*_test.go'` | `lancet fd '*_test.go'` | Patterns with `*`, `?`, `[` auto-switch to glob mode |
| `find . -mtime -7 -type f` | `lancet fd -t f --changed-within 7d` | Units: `s`, `m`, `h`, `d`, `w` |
| `find . -name '*.pdf'` | `lancet fd --type-class document` | 9 semantic classes instead of listing extensions manually |
| `fd config src` (if pattern matches dir name) | `lancet fd -p config src` | `-p`/`--pattern` disambiguates pattern from path |

## Coming from ast-grep (sg) {#from-ast}

| ast-grep | lancet ast | Key Difference |
| --- | --- | --- |
| `sg -p '$X.unwrap()' -l rs` | `lancet ast '$X.unwrap()' -l rs` | Accelerated by `.lancet` trigram prefilter on pattern literals |
| `sg -p 'f($$$A)' -r 'g($$$A)' -U` | `lancet ast 'f($$$A)' -r 'g($$$A)' --apply` | Dry-run diff until `--apply`; `--limit` refused with `--apply` |
| `sg -p 'if ($C) $S'` | `lancet ast 'if ($C) $S'` | **Strict AST shape:** matches plain `if` only; `if-else` is a separate node |
| `sg -l cpp ...` | `lancet ast ... -l cpp` | Lancet `cpp` scans `.h` headers alongside `.cpp`/`.cc` |

## Coming from cat / sed / patch / rga {#from-edit}

| Old Chain | Lancet Replacement | What Stops Breaking |
| --- | --- | --- |
| `sed -n '10,50p' f.rs` | `lancet read f.rs --lines 10:50` | Returns XXH3-64 `rev` and respects `--budget-tokens` |
| Two `sed -i` calls from one `rg -n` | `lancet edit f.rs --edits - --rev REV --apply` | Original coordinates for all hunks; preserves symlinks, hard links, mode |
| `patch -p1 < diff` | `lancet patch diff --apply` | All-file pre-verification, exact relocation, CAS race rollback |
| `rga` + `pdftotext`/`pandoc` | `lancet 'pat'` + `lancet read f.pdf -p 2` | Shared extraction means search hit line == read line |

## When NOT to Replace Specialized Tools {#when-not}

> [!WARNING] Intentional scope boundaries
> - **Version control:** keep using `git`; Lancet respects `.gitignore` and consumes `git diff` via `lancet patch`.
> - **Cross-file type-checked renames:** use your language's LSP (`rust-analyzer`, `tsserver`, `gopls`) for public APIs, fields, traits, or Python keyword parameters across files. Lancet intentionally restricts `rename` to scope-proven single-file local bindings.
