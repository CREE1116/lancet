---
nav_title: "CLI Commands & Exit Codes"
title: "CLI Subcommands, Flags & Exit Codes Reference"
description: "Every Lancet subcommand, flag default, argument rule, and exit code verified against src/main.rs."
lede: "Authoritative per-subcommand syntax and exit codes across all 15 entrypoints."
keywords: "cli subcommands flags exit codes 0 1 2 conflict invalid_query unsupported"
source: "https://github.com/CREE1116/lancet/blob/main/src/main.rs"
---
All subcommands share deterministic exit codes, `.gitignore` traversal, and structured JSON output when `--json` is passed. For `lancet rg`'s 40+ ripgrep flags, see [`lancet rg` Parity Reference](rg-parity.md).

## Exit Codes {#exit-codes}

| Code | Meaning | Returned JSON `status` |
| ---: | --- | --- |
| `0` | Success / at least one match / edit applied or previewed cleanly | `complete` |
| `1` | 0 matches (or missing index in `lancet status`) | `empty` |
| `2` | Bad query/CLI flags, unsupported target/rename, CAS conflict, or I/O error | `invalid_query`, `unsupported`, `conflict` |

## lancet <QUERY> & batch & parse & explain {#cmd-search}

| Command & Flag | Default / Values | Meaning |
| --- | --- | --- |
| `lancet <QUERY> [PATH]` | `PATH = .` | Evaluate LQL query over directory or file |
| `--json` · `--stats` | `false` | Machine-readable JSON · execution telemetry on stderr |
| `--no-index` \| `--trust-index` \| `--index-dir DIR` | `auto`, `PATH/.lancet` | Override index freshness strategy or location |
| `--hidden` · `--no-ignore` · `-j THREADS` · `--ocr` | `false`, CPUs, `false` | Traversal & OCR toggles |
| `--max-lines N` · `--max-cols N` | `20`, `240` | Display cap per matched unit and line |
| `lancet batch [PATH] -q Q... [--name N...] [--queries-json JSON]` | — | Share one index open + walk across multiple LQL queries |
| `lancet parse <QUERY>` | — | Print normalized canonical query + JSON IR |
| `lancet explain <QUERY> [PATH]` | — | Show required trigrams, candidate count & selectivity % |

## lancet fd & outline & refs & read {#cmd-discover}

| Subcommand | Flags & Arguments |
| --- | --- |
| `lancet fd [PATTERN] [PATH...]` | `-p/--pattern`, `-t f\|d`, `-e/--ext` (comma/repeatable), `--type-class`, `-g` (glob; auto-detected on `*`/`?`/`[`), `-r` (regex), `-s` (case), `-d/--max-depth`, `--changed-within DUR`, `--min-size`, `--max-size`, `--tree`, `--dirs`, `--limit N`, `--meta`, `--no-index`, `--trust-index`, `--index-dir`, `--hidden`, `--no-ignore`, `-j`, `--json`, `--stats` |
| `lancet outline [PATH]` | `-d/--max-depth`, `-e/--ext`, `--limit N`, `--hidden`, `--no-ignore`, `-j`, `--ocr`, `--json` |
| `lancet refs <SYMBOL> [PATH]` | `--rank`, `--kind KIND,...` (requires `--rank`), `-l/--lang`, `-e/--ext`, `--path GLOB`, `--all-files`, `--limit N`, `--no-index`, `--trust-index`, `--index-dir`, `--hidden`, `--no-ignore`, `-j`, `--json`, `--stats` |
| `lancet read <TARGET>...` | One or more targets (`file:10-50`, `zip!entry`), `-l/--lines`, `-s/--symbol`, `-p/--page`, `--sheet`, `--rows`, `--cols`, `-e/--entry`, `--budget-tokens`, `--raw`, `--ocr`, `--json` |

## lancet ast, edit, patch, rename, index & mcp {#cmd-mutate}

| Subcommand | Flags & Arguments |
| --- | --- |
| `lancet ast <PATTERN> [PATH]` | `-l/--lang`, `-r/--rewrite TEMPLATE`, `--apply`, `--path GLOB` (repeatable), `--limit N`, `--no-index`, `--trust-index`, `--index-dir`, `--hidden`, `--no-ignore`, `-j`, `--json`, `--stats` |
| `lancet edit <PATH>` | Target (one of): `--lines`, `--symbol`, `--find [--all]`, or `--edits FILE\|-` · Operation (one of): `--with`, `--with-file FILE\|-`, `--delete`, `--insert-before`, `--insert-after` · Safety: `--rev REV`, `--apply`, `--allow-syntax-errors`, `--json` |
| `lancet patch [DIFF\|-]` | `--root DIR` (`.`), `-p/--strip N` (`1`), `--apply`, `--allow-syntax-errors`, `--json` |
| `lancet rename <FILE:LINE[:COL]> <OLD> <NEW>` | `--rev REV`, `--apply`, `--json` |
| `lancet index [PATH]` · `status [PATH]` | `--rebuild` \| `--compact`, `--index-dir`, `--ocr`, `--hidden`, `--no-ignore`, `-j` |
| `lancet mcp` | `--root DIR` (`.`), `--read-only`, `--trust-index`, `-j/--threads` |
