---
nav_title: "Installation & Quickstart"
title: "Lancet Documentation — Installation & Cheat Sheet"
description: "Install Lancet on macOS, Linux, or Windows, run a 60-second walkthrough, and reference all 15 subcommands."
lede: "One Rust binary for discovery, trigram-indexed search, 24-language AST rewrites, exact reads, and CAS edits."
keywords: "install homebrew curl windows cargo binstall no-default-features cheat sheet walkthrough"
source: "https://github.com/CREE1116/lancet/blob/main/README.md"
---
**Lancet** replaces the `rg` + `fd` + `ast-grep` + `cat` + `sed` + `patch` + `pdftotext` chain with one dependency-free Rust binary. Every command shares the same `.lancet` trigram index, traversal filter rules, **1-based line coordinates**, and **XXH3-64 `rev`** file revisions.

> [!TIP] Design contract in one sentence
> The trigram index only decides which files *not* to open; candidates and live-changed files go through the exact same evaluator as a full scan, and every write runs through locked compare-and-swap.

## Installation & Build Variants {#install}

Prebuilt binaries are published on every release for macOS (Apple Silicon & Intel), Linux (x86_64/arm64 glibc and static musl), and Windows.

```bash title="macOS / Linux (Homebrew)"
brew tap CREE1116/lancet https://github.com/CREE1116/lancet
brew install CREE1116/lancet/lancet
```

```bash title="macOS / Linux (Prebuilt Release Script)"
curl -fsSL https://raw.githubusercontent.com/CREE1116/lancet/main/install.sh | sh
```

```bash title="Windows (PowerShell)"
irm https://raw.githubusercontent.com/CREE1116/lancet/main/install.ps1 | iex
```

```bash title="Cargo (Full vs Lean Code-Only Build)"
# Prebuilt binary via cargo-binstall
cargo binstall --git https://github.com/CREE1116/lancet lancet

# Compile full build (~40 MB: 24 Tree-sitter grammars + PDF/Office/HWP/archive adapters)
cargo install --git https://github.com/CREE1116/lancet lancet

# Lean build (~4 MB: pure source/text/code, strips document & archive adapters)
cargo install --git https://github.com/CREE1116/lancet lancet --no-default-features
```

## 60-Second End-to-End Walkthrough {#walkthrough}

Building `.lancet` is optional. Every command falls back seamlessly when no index exists, and default `auto` mode checks file metadata on disk on every query so edited files are never missed.

```bash title="Terminal"
# 1. Optional: build the local trigram index once
lancet index .

# 2. Orient with folder-only tree + usage-ranked core symbols
lancet fd --tree --dirs -d 2 .
lancet refs --rank '*Service' --kind class src --limit 8

# 3. Search with ripgrep flags or LQL boolean/proximity queries
lancet rg -w execute_one src
lancet 'near("gradient", "clip", 30) in function | expand parent'

# 4. Read exact symbol or lines and capture its XXH3-64 `rev`
lancet read src/search.rs --symbol execute_one --json

# 5. Preview dry-run diff, then commit atomically with --rev and --apply
lancet edit src/search.rs --find 'let limit = 20;' \
  --with 'let limit = 50;' --rev <REV> --apply
```

## One-Screen Command Cheat Sheet {#cheat-sheet}

| Workflow Step | Command | Safety / Performance Guarantee |
| --- | --- | --- |
| Folder layout | `lancet fd --tree --dirs -d 2 .` | Respects `.gitignore` / `.ignore` |
| Recent / typed files | `lancet fd -t f -e rs --changed-within 7d` | Units: `s`, `m`, `h`, `d`, `w` |
| Drop-in ripgrep | `lancet rg -w -t rs 'pat' src` | Byte-identical to `rg` + opt-in `--docs` |
| Boolean / fuzzy LQL | `lancet '("a" OR "b") NOT "c" in function'` | `Scan == Auto` |
| Multi-query batch | `lancet batch src -q 'Q1' -q 'Q2'` | One index open & directory walk |
| Skeleton outline | `lancet outline src/main.rs` | Functions, MD headings, sheets, ZIP entries |
| Budgeted read | `lancet read f.rs --lines 10:50 --budget-tokens 800` | Returns `rev` + `next_line` cursor |
| Symbol read | `lancet read f.rs --symbol my_fn` | Includes doc comments & attributes |
| Document / archive | `lancet read r.pdf -p 2:3` · `a.zip!app.log` | 1-to-1 line numbers with search; read-only |
| Ranked declarations | `lancet refs --rank '*Service' --kind class src` | Sorted by lexical AST reference count |
| AST search/rewrite | `lancet ast '$X.unwrap()' src -l rs -r '$X?'` | 24 languages; dry-run until `--apply` |
| Atomic multi-edit | `lancet edit f.rs --edits - --rev REV --apply` | Original coordinates, overlap-checked CAS |
| Unified diff patch | `git diff \| lancet patch --apply` | All-file pre-check + CAS rollback |
| Local binding rename | `lancet rename f.rs:42 old new --rev REV --apply` | 6 languages; scope re-resolution proof |
| MCP stdio server | `lancet mcp --root .` | one `lancet` tool taking CLI arguments, JSON-RPC 2.0, `--read-only` option |
| Index diagnostics | `lancet explain 'pat'` · `lancet status` | Trigram requirements & selectivity % |

## Where to Go Next {#next-steps}

:::cards
- Getting Started | Migration & Filtering Rules | Exact Rosetta Stone from rg/fd/sg/sed/patch plus how .gitignore, globs, and file types evaluate. | migration.md
- Search & Discovery | Explore, LQL, AST & Documents | Low-context exploration funnel, boolean/proximity DSL, 24-language AST rewrites, PDF/HWP/ZIP reading. | guides/explore.md
- Editing & Agents | Edit, Patch, Rename & MCP | Locked --rev CAS editing, multi-file patch rollback, tier-1 scope-proven rename, MCP setup. | guides/edit.md
- Internals & Reference | LANCET04, Write Safety & Schemas | Index segment format, racy-timestamp rules, OS locking, CLI flags, JSON structs, and troubleshooting. | reference/cli.md
:::
