---
nav_title: "Files, Outlines & Ranking"
title: "Discovering Files, Outlines & Symbol Usage Ranking"
description: "Use lancet fd --dirs --tree, lancet outline, lancet refs, and lancet refs --rank to map repositories quickly."
lede: "Turn an unfamiliar repository into a usage-ranked map of directories, declarations, and sheets before opening full files."
keywords: "fd tree dirs changed-within meta outline refs rank kinds declarations definitions"
source: "https://github.com/CREE1116/lancet/blob/main/docs/agent-instructions.md"
---
Reading full files burns context tokens fast. Use Lancet's discovery commands — `fd`, `outline`, and `refs` — to turn an unfamiliar tree into a ranked map of exact files, declarations, and line bounds.

## Fast Tree & Metadata Discovery (lancet fd) {#fd-guide}

`lancet fd` takes fd's command line: the pattern is a regex matched against names (smart case), `-g` makes it a glob, and paths come after the pattern. It adds `--tree`, and with `--trust-index` it reads the file list straight from the `.lancet` index. Every flag is in the [fd parity reference](../reference/fd-parity.md).

```bash title="Terminal"
# Compact folders-only architecture map, or several roots at once
lancet fd --tree --dirs --max-depth 2 .
lancet fd --tree -d 2 . src docs
lancet fd . crates/core crates/cli

# Regex by default, globs with -g, extensions with -e
lancet fd '_test\.rs$' crates/
lancet fd -g '*_test.rs' crates/
lancet fd -e proto

# Recency + size bounds (durations as in fd: 48h, 2weeks; sizes: k = 1000, ki = 1024)
lancet fd -t f --changed-within 48h -S +4k -S -2m

# JSON with file sizes and mtime_ns (costs 1 stat per returned entry)
lancet fd -e rs --meta --max-results 25 --json . src
```

> [!NOTE] Exit codes are fd's
> `lancet fd` exits `0` even when nothing matched (as fd does), `1` for its own errors such as a missing search path, and `2` for invalid arguments. Use `-q` for a yes/no answer.

## Declaration, Heading & Archive Skeletons (lancet outline) {#outline-guide}

Inspect structure before opening lines. `lancet outline` understands AST declarations (code), Markdown headings, PDF/PPTX/DOCX/HWP headings, spreadsheet sheet names + dimensions, and archive entry manifests:

```bash title="Terminal"
# Single file AST skeleton with line spans & signatures
lancet outline src/scope.rs

# Directory outline capped to depth & extension
lancet outline src -d 2 -e rs --limit 20 --json

# Non-code outlines: Markdown TOC, workbook sheets, zip/tar entries
lancet outline docs/spec.md
lancet outline financial_model.xlsx
lancet outline logs/crash_bundle.tar.gz
```

## Lexical AST References & Usage Ranking (lancet refs) {#refs-and-rank}

Unlike plain `rg`, `lancet refs` parses candidate files with Tree-sitter across all 24 supported languages, separates `[def]` declarations from `[ref]` identifier sites, records the enclosing symbol, and **excludes comments and string literals**.

```bash title="Terminal"
# Find definitions and all lexical call/reference sites
lancet refs SearchOptions src --lang rs

# Also scan non-code files (Markdown, TOML, configs) with lexical matching
lancet refs SearchOptions . --all-files
```

### Finding Core Symbols with refs --rank {#rank-mode}

Pass `--rank` to treat the first argument as a glob pattern over declaration names (for example `*Service` or `*Handler`) and sort matching declarations by lexical reference count descending. When the glob contains a literal run (like `Service` in `*Service`), Lancet accelerates file selection with the trigram index.

```bash title="Terminal"
# Most-referenced *Service classes in src/
lancet refs --rank '*Service' --kind class src --limit 10

# Top functions & structs in a Rust crate
lancet refs --rank '*' --kind function,struct src --limit 15 --json
```

If several declarations share the same name (e.g. `new` or `run`), `lancet refs --rank` prints `(name shared by N declarations)` and sets `definitions: N` in JSON so you know the lexical count aggregates references across overloaded or same-named definitions.
