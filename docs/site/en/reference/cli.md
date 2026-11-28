---
nav_title: "CLI Commands & Exit Codes"
title: "CLI Commands, Flags & Exit Codes Reference"
description: "Every Lancet command, its flags and defaults, and the exit codes they share."
lede: "Twelve commands, one set of exit codes, one coordinate system."
keywords: "cli commands flags exit codes 0 1 2 conflict invalid_query unsupported"
source: "https://github.com/CREE1116/lancet/blob/main/src/commands.rs"
---
`lancet --help` lists the commands and `lancet COMMAND --help` lists each one's flags; both come from the same command table as this page (`src/commands.rs`). For `lancet rg`'s ripgrep flags, see the [`lancet rg` parity reference](rg-parity.md).

## Exit codes {#exit-codes}

| Code | Meaning | JSON `status` |
| ---: | --- | --- |
| `0` | Done: matches found, or a change previewed or applied | `complete` |
| `1` | Nothing found (not an error) | `empty` |
| `2` | Refused or invalid: bad arguments, unsupported file, a conflict (the file changed since its `rev`), I/O error | `invalid_query`, `unsupported`, `conflict` |

`lancet fd` keeps fd's own codes: `0` even when nothing matched, `1` for its errors (a missing search path), `2` for invalid arguments.

With `--json`, every command's response has `status`, and `error` when it failed; searching commands also report `warnings`.

## Find and search {#cmd-search}

| Command | Flags |
| --- | --- |
| `lancet rg [FLAGS] PATTERN [PATH...]` | ripgrep's flags ([parity](rg-parity.md)), plus `--docs` (search documents and archives) |
| `lancet fd [PATTERN] [PATH...]` | fd's flags ([parity](fd-parity.md)): `-g`, `-F`, `--exact`, `-p`, `-s`/`-i`, `-t`, `-e`, `-S`, `--changed-within`/`--changed-before`, `-o`, `-d`/`--min-depth`, `-E`, `-H`, `-I`, `-u`, `-a`, `-0`, `--format`, `--max-results`, `-q` … · lancet: `--tree`, `--dirs`, `--type-class`, `--json`, `--meta` |
| `lancet refs SYMBOL [PATH...]` | `--rank` (SYMBOL is a glob; most referenced declarations first), `--kind KIND,...`, `-l/--lang`, `-e/--ext`, `--path GLOB`, `--all-files`, `--limit N`, `--hidden`, `--no-ignore`, `-j`, `--json` |
| `lancet ast PATTERN [PATH...]` | `-l/--lang`, `-r/--rewrite TEMPLATE`, `--apply`, `--allow-syntax-errors`, `--path GLOB`, `--limit N`, `--hidden`, `--no-ignore`, `-j`, `--json` ([pattern syntax](ast-patterns.md)) |

Searching commands use the index automatically when one exists; `--no-index` and `--trust-index` choose the mode explicitly (see [index and freshness](../concepts/index-and-freshness.md)).

## Read {#cmd-read}

| Command | Flags |
| --- | --- |
| `lancet read TARGET...` | Targets: `file`, `file:10-50`, `file:42`, `archive.zip!inner.log`, several per call · `-l/--lines`, `--offset N --limit N`, `-s/--symbol`, `-p/--page`, `--sheet`, `--rows`, `--cols`, `-e/--entry`, `--budget-tokens N`, `--raw`, `--ocr`, `--json` |
| `lancet outline [PATH...]` | `-d/--max-depth`, `-e/--ext`, `--limit N`, `--hidden`, `--no-ignore`, `-j`, `--ocr`, `--json` |

`read` prints at most 2,000 lines unless you give a range; the first line is `# PATH rev REV`.

## Change {#cmd-change}

Every change is a preview until `--apply`; every write is a compare-and-swap and multi-file changes are all or nothing ([write safety](../concepts/write-safety.md)).

| Command | Flags |
| --- | --- |
| `lancet edit PATH` | Target: `--old-string TEXT` (`--replace-all`, `--expected-replacements N`), `--lines`, `--symbol` · Operation: `--new-string TEXT`, `--with-file FILE\|-`, `--delete`, `--insert-before`, `--insert-after` · `--edits FILE\|-` (several hunks or files at once) · `--rev REV`, `--apply`, `--allow-syntax-errors`, `--json` |
| `lancet write PATH` | `--content TEXT` or `--with-file FILE\|-`, `--rev REV` (required to replace an existing file), `--apply`, `--allow-syntax-errors`, `--json` |
| `lancet patch [DIFF\|-]` | Unified diff or Codex `*** Begin Patch` · `--root DIR` (`.`), `-p/--strip N` (`1`), `--apply`, `--allow-syntax-errors`, `--json` |
| `lancet rename FILE:LINE[:COL] OLD NEW` | `--rev REV`, `--apply`, `--json` |

## Infrastructure {#cmd-infra}

| Command | Flags |
| --- | --- |
| `lancet index [PATH]` | `--status` (print the index's state as JSON, change nothing), `--rebuild`, `--compact`, `--index-dir DIR`, `--hidden`, `--no-ignore`, `--ocr`, `-j` |
| `lancet mcp` | `--root DIR` (`.`), `--read-only` (refuse `edit`, `write`, `patch`, `rename`, `index` and `--apply`), `--trust-index`, `-j` |
