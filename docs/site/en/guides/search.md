---
nav_title: "Text Search (lancet rg)"
title: "Text Search: lancet rg on an Always-Fresh Index"
description: "ripgrep's flags, output and exit codes on lancet's trigram index: index modes, several paths, --in/--in0, documents with --docs."
lede: "Type the ripgrep command you already know; the index makes it faster without changing a byte of the output."
keywords: "rg ripgrep regex index trust-index no-index docs in in0 max-bytes parity"
source: "https://github.com/CREE1116/lancet/blob/main/docs/rg-parity.md"
---
`lancet rg` is ripgrep: the same flags, the same Rust `regex` engine, the same traversal rules (`.gitignore`, `.ignore`, `.rgignore`, `--ignore-file`), the same output and the same exit codes (`0` match, `1` no match, `2` error). The only difference is underneath: when a `.lancet` index exists, files that cannot match are never opened.

## Everyday searches {#recipes}

```bash title="Terminal"
lancet rg -w spin_lock src                 # whole word
lancet rg -n -C 2 'fn \w+_locked\(' src    # regex with context
lancet rg -i -l 'connection reset' .       # files only, case-insensitive
lancet rg -t rs -g '!*_test.rs' unwrap .   # types and globs
lancet rg --json 'TODO' src                # ripgrep's JSON events
lancet rg -c ThreadPoolBuilder a/ b/ c.rs  # several paths in one call
```

Every supported flag and its exact behaviour is listed in the [ripgrep parity reference](../reference/rg-parity.md). Output is compared with ripgrep 16.0 byte for byte on 150 cases in CI.

## How the index is used {#index-modes}

| Mode | Flag | What happens |
| --- | --- | --- |
| auto | (default) | The regex is turned into a trigram formula; only files that can contain it are read. Files changed since the index was built (`mtime`, size, `ctime`, inode) are read directly, so results are always fresh. |
| scan | `--no-index` | No index: a plain parallel scan, within a few percent of ripgrep. |
| trust | `--trust-index` | Skips the freshness check. Fastest; an explicit choice, never a silent default. |

Building the index is optional (`lancet index .`). Without one, `lancet rg` behaves exactly like ripgrep. The index only ever *removes* files that cannot match: when lancet cannot derive a requirement from a pattern (for example `\w+`), it reads every file rather than guess.

## Paths from a file: --in and --in0 {#in-files}

```bash title="Terminal"
git diff --name-only | lancet rg --in - 'unsafe'
fd -0 -e rs | lancet rg --in0 - 'unsafe'
```

Listed paths keep the order you gave, mixed with positional paths exactly as ripgrep 16 orders them.

## Documents and archives: --docs {#docs}

By default `lancet rg` searches what ripgrep searches, so its output stays identical. Add `--docs` to also search inside PDFs, Office files, HWP/HWPX, EPUB and archives. Line numbers are those of `lancet read`, so a hit can be read back directly:

```bash title="Terminal"
lancet rg --docs -n 'indemnity' contracts/
lancet read contracts/msa.docx:120-140
```

## What is not supported {#unsupported}

`-P` / PCRE2 (look-around, backreferences) is refused: the Rust engine keeps matching linear-time and lets the index derive a sound filter from every pattern. `--pre` with a command is refused: lancet doesn't run a process per file, and reads documents and archives in-process with `--docs`. See the [parity reference](../reference/rg-parity.md) for the full list.
