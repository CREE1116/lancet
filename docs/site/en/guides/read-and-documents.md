---
nav_title: "Reads, PDF, Office & HWP"
title: "Exact Reads, PDFs, Office, HWP, Archives & OCR"
description: "Read lines, symbols, token-budget slices, PDF/PPTX pages, spreadsheets, HWP/HWPX, ZIP/TAR entries, and scanned OCR."
lede: "Shared 1-to-1 line coordinates across code and documents, with 16 MiB decompression caps and read-only safety."
keywords: "read budget-tokens next_line pdf docx pptx hwp hwpx xlsx zip tar ocr 16 mib decompression"
source: "https://github.com/CREE1116/lancet/blob/main/docs/agent-instructions.md"
---
`lancet read` extracts exact slices from source code, PDFs, Office suites, Korean HWP/HWPX documents, and compressed archives — returning the **exact same 1-based line numbers** reported by search.

## Reading Code: Lines, Symbols & Token Budgets {#code-reading}

```bash title="Terminal"
# Inline line range (supports file:10-50, file:42, --lines 100:)
lancet read src/search.rs:100-160

# Read several slices in one call (v0.1.4+; each prints its own `# PATH rev REV` header)
lancet read src/search.rs:100-140 src/main.rs:1-40

# Exact Tree-sitter declaration (INCLUDES leading doc comments & attributes)
lancet read src/search.rs --symbol execute_one --json

# Cap response by estimated tokens and paginate cleanly via `next_line`
lancet read src/mcp.rs --lines 1:800 --budget-tokens 800 --json

# Omit header/gutter for raw copy/piping
lancet read src/lib.rs --lines 1:40 --raw
```

Every plain-text response includes a 16-hex **`rev`** (XXH3-64 hash of disk bytes) in the header comment and JSON field. When reading multiple targets with `--json`, Lancet returns `{"status": ..., "results": [ReadResponse, ...]}` (with exit code equal to the worst status across targets). Pass `rev` straight to [`lancet edit --rev`](edit.md) or [`lancet rename --rev`](rename.md).

## Supported Documents, Spreadsheets & Archives {#formats}

| Format | Extensions | Read Selectors | Notes |
| --- | --- | --- | --- |
| PDF | `.pdf` | `--page 2` · `--page 2:5` · `--lines` | In-process Pure-Rust extraction; `--ocr` optional |
| Word / ODT / EPUB | `.docx`, `.odt`, `.epub` | `--lines START:END` | XML paragraph stream extraction |
| PowerPoint | `.pptx` | `--page 3` · `--page 1:4` | Maps slides to `[Slide N]` sections |
| Hangul (HWP) | `.hwp` (5.0 OLE), `.hwpx` | `--lines START:END` | Native binary OLE & XML Hangul extraction |
| Spreadsheets | `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods` | `--sheet S` · `--rows 1:30` · `--cols A:E` | `--sheet` takes sheet name or 1-based index |
| Archives | `.zip`, `.tar`, `.gz`, `.tgz`, `.tar.gz` | `bundle.zip!inner/file.log` or `--entry` | Pair with `--lines` & `--budget-tokens` |
| Images (with `--ocr`) | `.png`, `.jpg`, `.jpeg`, `.tif`, `.tiff`, `.bmp`, `.webp` | `--ocr` | Requires external `tesseract` on `PATH` |

## Searching vs Reading Non-Text Files {#rg-docs}

- **`lancet '<LQL>'`:** indexes and searches document/archive adapters automatically (filter with `type:document`, `type:spreadsheet`, or `type:archive`).
- **`lancet rg --docs`:** adapters are OFF by default in `lancet rg` so plain runs stay byte-identical to `ripgrep`; pass `--docs` to include extracted document and archive text in `lancet rg`.

```bash title="Terminal"
# Find hitting sheet/row first, then slice exact columns
lancet '"EBITDA" in type:spreadsheet' ./finance
lancet read finance/q4.xlsx --sheet 1 --rows 12:35 --cols A:F

# List archive entries via outline, then stream just one log
lancet outline artifacts/bundle.zip
lancet read artifacts/bundle.zip!worker_0/panic.log --lines 1:80 --ocr
```

## Safety Caps, Panic Isolation & Read-Only Contract {#caps-safety}

- **Hard resource limits (`src/adapters.rs`):** input file size capped at **64 MiB** (`MAX_INPUT_BYTES`), extracted UTF-8 buffer capped at **16 MiB** (`MAX_EXTRACTED_BYTES`) safely truncated on UTF-8 char boundaries, and archive traversal capped at **500 entries** (`MAX_ARCHIVE_ENTRIES`) to defeat decompression bombs.
- **Panic isolation:** malformed third-party PDF/document panics are caught inside `catch_unwind` and treated cleanly as unreadable rather than crashing search/index workers.
- **Read-only by contract:** binary documents and archive members omit `rev` in JSON, and `lancet edit` refuses to write back to them.
- **Scanned PDFs:** pass `--ocr` (requires `pdftoppm` + `tesseract` on `PATH`) on `lancet`, `read`, `outline`, or `index`.
