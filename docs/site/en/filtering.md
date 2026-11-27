---
nav_title: "Ignore Rules, Globs & Types"
title: "Ignore Rules, Globs, File Types & Traversal Bounds"
description: "How .gitignore, .ignore, .rgignore, -u/-uu/-uuu, --glob, --type-add, type:, -L, and --max-depth work."
lede: "Consistent traversal and file classification rules shared across search, fd, outline, refs, ast, and index."
keywords: "gitignore rgignore hidden binary unrestricted -u -uu -uuu glob type-add max-depth max-filesize symlinks"
source: "https://github.com/CREE1116/lancet/blob/main/docs/rg-parity.md"
---
Lancet uses ripgrep's `ignore` crate traversal rules uniformly across `lancet`, `rg`, `fd`, `outline`, `refs`, `ast`, and `index` so discovery, search, and indexing agree on which files exist.

## Ignore Precedence, Hidden & Binary Files {#ignore-files}

- **Ignore stack:** honors `.gitignore`, global/repo git excludes, `.ignore`, and `.rgignore` (unless `--no-ignore` or `-u` is set).
- **Hidden dotfiles:** files/directories starting with `.` (including `.lancet`) are skipped by default; pass `--hidden` / `-.` (or `-uu` in `lancet rg`) to include them.
- **Binary detection:** plain-text search skips binary files containing NUL bytes unless `-a` / `--text` / `-uuu` is passed, or a document/archive adapter handles the extension.
- **Positive glob override:** in `lancet rg`, positive globs like `-g '*.lock'` override ignore rules and hidden status identically to ripgrep.

## Unrestricted Escalation (-u / -uu / -uuu) in lancet rg {#rg-unrestricted}

| Flag | Equivalent Flags | Effect |
| --- | --- | --- |
| `-u` | `--no-ignore` | Search ignored files (build artifacts, `node_modules`, gitignored logs) |
| `-uu` | `--no-ignore --hidden` | Also search hidden dotfiles and directories |
| `-uuu` | `--no-ignore --hidden --text` | Also search binary files as raw text |

## Globs, Built-in Types & --type-add {#globs-types}

```bash title="Terminal"
# Include & exclude globs (! negates; case-insensitive --iglob also supported)
lancet rg -g 'src/**' -g '!src/generated/**' 'retry' .

# Restrict to / exclude ripgrep built-in file types
lancet rg -t rust -T test 'unsafe' .

# Define custom types or merge existing ones (repeatable)
lancet rg --type-add 'web:*.{ts,tsx,css}' -t web 'ThemeProvider' .
lancet rg --type-add 'jvm:include:java,kotlin,scala' -t jvm 'Executor' .

# Inspect all known definitions (including your --type-add)
lancet rg --type-add 'proto:*.proto' --type-list
```

## LQL & fd Filters (type:, ext:, path:, --type-class) {#lql-filters}

In LQL queries, multiple values inside a category are **OR-ed**, while different categories are **AND-ed**. Notice a subtle rule on `path:` implemented in `src/walk.rs`: when a `path:` value has no glob metacharacters (`*`, `?`, `[`, `{`), Lancet automatically matches both `dir` and `dir/**`.

```bash title="Terminal"
# OR within ext:(...), AND across type: and path:
lancet '"deadline" in ext:(rs,toml) path:crates/core'

# Discover files by Lancet semantic class
lancet fd --type-class document,spreadsheet --changed-within 14d
```

| Class (`type:` / `--type-class`) | Covered Extensions / Examples |
| --- | --- |
| `code` | Source files across 24+ programming languages (`.rs`, `.py`, `.ts`, `.go`, `.c`, `.cpp`, …) |
| `document` | `.md`, `.txt`, `.rst`, `.pdf`, `.docx`, `.pptx`, `.odt`, `.hwp`, `.hwpx`, `.epub` |
| `spreadsheet` | `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods`, `.csv`, `.tsv` |
| `config` | `.toml`, `.yaml`, `.yml`, `.ini`, `.env`, `.hcl`, `.tf` |
| `data` | `.json`, `.jsonc`, `.xml`, `.sql`, etc. |
| `log` · `archive` · `image` · `other` | `.log` · `.zip/.tar/.gz/.tgz` · `.png/.jpg/.webp/…` · remaining text/files |

## Symlinks, Depth & File-Size Bounds {#bounds}

- **Symlinks (`-L` / `--follow`):** disabled by default. On `lancet rg -L --trust-index`, Lancet deliberately falls back to a live filesystem walk so symlinked targets absent from `.lancet` are never skipped.
- **Depth & size:** `--max-depth N` (`1` = direct children only) and `--max-filesize NUM[K|M|G]` (in `rg`) or `--min-size`/`--max-size` (in `fd`) are enforced in both live walks and `--trust-index`.
- **Multi-path `fd` & missing paths (v0.1.4):** `lancet fd [PATTERN] [PATH...]` accepts multiple paths (`lancet fd . a b` lists all entries under `a` and `b`). A nonexistent `PATH` across `fd`, `search`, `batch`, `refs`, and `ast` exits `2` with `PATH: No such file or directory` on stderr instead of returning silent `empty` (exit `1`). On `fd`, a lone argument containing `/` and no glob characters is treated as a path so typos like `lancet fd docs/imgaes` fail loudly.
