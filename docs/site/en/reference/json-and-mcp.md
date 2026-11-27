---
nav_title: "JSON Schemas & MCP Tools"
title: "JSON Response Schemas & MCP 11-Tool Reference"
description: "Exact JSON response structs and all 11 lancet mcp inputSchema parameters, defaults, and JSON-RPC codes."
lede: "Machine-readable response fields verified against Rust structs and every MCP tool parameter schema."
keywords: "json schema response mcp inputschema lancet_fd lancet_rg lancet_edit max_output_bytes json-rpc"
source: "https://github.com/CREE1116/lancet/blob/main/docs/mcp.md"
---
Exact JSON response types serialized from Rust structs in `src/` and complete parameter schemas for all 11 tools in `src/mcp.rs`.

## Status Enum & ErrorInfo {#status}

- `Status`: `"complete"` | `"empty"` | `"invalid_query"` | `"unsupported"` | `"conflict"`
- `ErrorInfo`: `{"message": string, "pos"?: number}` (`pos` populated on LQL parse failures)

## CLI --json Response Payloads {#response-structs}

```bash title="Search / Read / Edit / Rename"
// Response (lancet '<QUERY>' --json)
{
  "status": "complete", "query": ""spin_lock" in file", "mode": "auto",
  "matched": 7073, "matched_is_lower_bound": false, "returned": 20, "truncated": true,
  "results": [{
    "id": "...", "path": "kernel/locking/spinlock.c", "unit": "file",
    "start_line": 12, "end_line": 44, "symbol": "raw_spin_lock",
    "lines": [{ "line": 12, "text": "...", "context": false }], "more_lines": 0
  }],
  "terms": [{ "term": "spinlock", "count": 42, "files": 19 }],
  "budget": { "limit_tokens": 1500, "used_tokens": 1120, "estimated": true },
  "stats": { "files_considered": 86218, "files_searched": 7814, "files_matched": 7073, "index_candidates": 7814, "stale_files": 0, "elapsed_ms": 141.3 }
}

// ReadResponse (rev omitted on binary documents & archives; multiple targets return { status, results: [ReadResponse, ...] })
{ "status": "complete", "path": "src/net.rs", "rev": "e70d5a898f122a62", "section": "fn retry_limit", "total_lines": 80, "start_line": 1, "end_line": 4, "returned_lines": 4, "truncated": false, "next_line": null, "estimated_tokens": 28, "lines": [{ "num": 1, "text": "..." }], "elapsed_ms": 0.4 }

// EditResponse (mode: "dry-run" | "applied")
{ "status": "complete", "mode": "applied", "path": "src/net.rs", "rev": "e70d5a898f122a62", "rev_after": "1486bdffbc06c68f", "start_line": 2, "end_line": 2, "hunks": [[2, 2]], "applied": true, "diff": "...", "elapsed_ms": 1.1 }

// RenameResponse (tier: "local-scope")
{ "status": "complete", "mode": "applied", "path": "src/search.rs", "old": "old_n", "new": "new_n", "tier": "local-scope", "rev": "...", "rev_after": "...", "declaration": { "line": 40, "col": 9 }, "sites": [{ "line": 40, "col": 9 }], "comment_mentions": 0, "applied": true, "elapsed_ms": 2.3 }
```

```bash title="Fd / Outline / Refs / Rank / Ast / Patch / Explain"
// FdResponse:      { status, mode, total, returned, truncated, entries: [{ path, kind, file_type?, size?, mtime_ns? }], elapsed_ms }
// OutlineResponse: { status, files: [{ path, file_type, total_lines?, items: [{ kind, name, depth, start_line?, end_line?, signature?, detail? }] }], total_items, truncated, elapsed_ms }
// RefsResponse:    { status, mode, symbol, definitions: [RefHit], references: [RefHit], total_references, truncated, stats }
// RankResponse:    { status, mode, pattern, entries: [{ name, kind, path, line, references, definitions }], total, truncated, files_parsed, elapsed_ms }
// AstResponse:     { status, mode, lang?, pattern, matched, returned, truncated, matches: [AstMatch], diffs: [FileDiff], stats }
// PatchResponse:   { status, mode, applied, files: [{ path, action, from?, hunks, offsets, rev?, rev_after?, diff }], elapsed_ms }
// ExplainResponse: { status, canonical?, ir?, trigram_req?, index_exists, files_considered, index_candidates?, stale_files?, selectivity_pct?, grams_available?, grams_decoded?, elapsed_ms }
```

## MCP 11-Tool Schemas (lancet mcp) {#mcp-tools}

Protocol revisions: `2025-03-26`, `2024-11-05`. JSON-RPC errors: `-32700` (parse error), `-32600` (invalid request), `-32601` (unknown method), `-32602` (invalid params / unknown prompt). Note: `status: "empty"` yields `isError: false`.

| Tool (Required) | Parameters & Types | Read-Only / Notes |
| --- | --- | --- |
| `lancet_fd` | `path`, `pattern`, `pattern_mode` (`substring\|glob\|regex`), `case_sensitive`, `kinds`, `dirs`, `exts`, `type_classes`, `max_depth`, `changed_within`, `min_size`, `max_size`, `tree`, `limit`, `index_mode` (`auto\|off\|trust`), `hidden`, `no_ignore`, `meta` | `readOnlyHint: true` |
| `lancet_search` (`query`) | `query`*, `path`, `max_lines` (20), `max_cols` (240), `index_mode`, `hidden`, `no_ignore`, `ocr` | `readOnlyHint: true` |
| `lancet_rg` (`pattern`) | `pattern`*, `path`, `fixed_strings`, `ignore_case`, `smart_case`, `word`, `line_regexp`, `invert`, `multiline`, `multiline_dotall`, `only_matching`, `count`, `files_with_matches`, `max_count`, `before`, `after`, `context`, `globs`, `iglobs`, `types`, `types_not`, `type_add`, `max_depth`, `max_filesize`, `follow`, `docs`, `passthru`, `index_mode`, `hidden`, `no_ignore`, `max_output_bytes` (128 KiB..4 MiB) | `readOnlyHint: true`; line-boundary truncation |
| `lancet_outline` | `path`, `max_depth`, `exts`, `limit`, `hidden`, `no_ignore`, `ocr` | `readOnlyHint: true` |
| `lancet_read` (`target`) | `target`*, `lines`, `symbol`, `page`, `sheet`, `rows`, `cols`, `entry`, `budget_tokens`, `ocr` | `readOnlyHint: true`; returns `rev` on plain text |
| `lancet_refs` (`symbol`) | `symbol`*, `path`, `lang`, `exts`, `paths`, `all_files`, `rank`, `kinds`, `limit`, `index_mode`, `hidden`, `no_ignore` | `readOnlyHint: true` |
| `lancet_ast` (`pattern`) | `pattern`*, `path`, `lang`, `rewrite`, `apply` (`false`), `paths`, `limit`, `index_mode`, `hidden`, `no_ignore` | `apply: true` refused under `--read-only` |
| `lancet_edit` (`path`) | `path`*, `lines`, `symbol`, `find`, `all`, `with`, `delete`, `insert_before`, `insert_after`, `edits` (array), `rev`, `apply` (`false`), `allow_syntax_errors` | Omitted under `--read-only` |
| `lancet_patch` (`patch`) | `patch`*, `root`, `strip` (`1`), `apply` (`false`), `allow_syntax_errors` | Omitted under `--read-only` |
| `lancet_rename` (`path,line,old,new`) | `path`*, `line`*, `col`, `old`*, `new`*, `rev`, `apply` (`false`) | Omitted under `--read-only` |
| `lancet_index` | `action` (`status\|sync\|rebuild\|compact`), `path`, `hidden`, `no_ignore`, `ocr` | `sync`/`rebuild`/`compact` refused under `--read-only` |
