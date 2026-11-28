---
nav_title: "JSON Schemas & MCP"
title: "JSON Response Schemas & the MCP Tool"
description: "Exact JSON response structs, the lancet mcp tool schema, and JSON-RPC codes."
lede: "Machine-readable response fields verified against Rust structs, and the single MCP tool that takes CLI arguments."
keywords: "json schema response mcp inputschema args stdin max_output_bytes json-rpc"
source: "https://github.com/CREE1116/lancet/blob/main/docs/mcp.md"
---
Exact JSON response types serialized from Rust structs in `src/`, and the schema of the one MCP tool in `src/mcp.rs`.

## Status Enum & ErrorInfo {#status}

- `Status`: `"complete"` | `"empty"` | `"invalid_query"` | `"unsupported"` | `"conflict"`
- `ErrorInfo`: `{"message": string, "pos"?: number}` (`pos`: a byte offset, when the error points into the input)

## CLI --json Response Payloads {#response-structs}

`lancet rg --json` prints ripgrep's JSON event stream unchanged (`begin`, `match`, `context`, `end`, `summary`). The other commands print one object:

```bash title="Refs / Read / Edit / Rename"
// RefsResponse (lancet refs plan_targets_with_req src --json)
{ "status": "complete", "mode": "index", "symbol": "plan_targets_with_req",
  "definitions": [{ "path": "src/scope.rs", "line": 225, "col": 4, "kind": "function", "text": "fn plan_targets_with_req(", "ast_verified": true }],
  "references": [{ "path": "src/scope.rs", "line": 152, "col": 36, "kind": "ref", "enclosing": "plan", "text": "targets = Some(plan_targets_with_req(", "ast_verified": true }],
  "total_references": 1, "truncated": false,
  "stats": { "files_considered": 41, "index_candidates": 0, "stale_files": 39, "files_scanned": 1, "elapsed_ms": 27.0 } }

// ReadResponse (rev omitted on binary documents & archives; multiple targets return { status, results: [ReadResponse, ...] })
{ "status": "complete", "path": "src/net.rs", "rev": "e70d5a898f122a62", "section": "fn retry_limit", "total_lines": 80, "start_line": 1, "end_line": 4, "returned_lines": 4, "truncated": false, "next_line": null, "estimated_tokens": 28, "lines": [{ "num": 1, "text": "..." }], "elapsed_ms": 0.4 }

// EditResponse (mode: "dry-run" | "applied")
{ "status": "complete", "mode": "applied", "path": "src/net.rs", "rev": "e70d5a898f122a62", "rev_after": "1486bdffbc06c68f", "start_line": 2, "end_line": 2, "hunks": [[2, 2]], "applied": true, "diff": "...", "elapsed_ms": 1.1 }

// RenameResponse (tier: "local-scope")
{ "status": "complete", "mode": "applied", "path": "src/scope.rs", "old": "old_n", "new": "new_n", "tier": "local-scope", "rev": "...", "rev_after": "...", "declaration": { "line": 40, "col": 9 }, "sites": [{ "line": 40, "col": 9 }], "comment_mentions": 0, "applied": true, "elapsed_ms": 2.3 }
```

```bash title="Fd / Outline / Refs / Rank / Ast / Patch"
// FdResponse:      { status, mode, total, returned, truncated, entries: [{ path, kind, file_type?, size?, mtime_ns? }], elapsed_ms }
// OutlineResponse: { status, files: [{ path, file_type, total_lines?, items: [{ kind, name, depth, start_line?, end_line?, signature?, detail? }] }], total_items, truncated, elapsed_ms }
// RefsResponse:    { status, mode, symbol, definitions: [RefHit], references: [RefHit], total_references, truncated, stats }
// RankResponse:    { status, mode, pattern, entries: [{ name, kind, path, line, references, definitions }], total, truncated, files_parsed, elapsed_ms }
// AstResponse:     { status, mode, lang?, pattern, matched, returned, truncated, matches: [AstMatch], diffs: [FileDiff], warnings, error?, stats }
// PatchResponse:   { status, mode, applied, files: [{ path, action, from?, hunks, offsets, rev?, rev_after?, diff }], elapsed_ms }
```

## MCP: the `lancet` tool {#mcp-tools}

Protocol revisions: `2025-03-26`, `2024-11-05`. JSON-RPC errors: `-32700` (parse error), `-32600` (invalid request), `-32601` (unknown method), `-32602` (unknown tool). One tool:

```json title="inputSchema"
{
  "type": "object",
  "properties": {
    "args":             { "type": "array", "items": { "type": "string" }, "minItems": 1 },
    "stdin":            { "type": "string" },
    "max_output_bytes": { "type": "integer", "minimum": 256, "default": 131072 }
  },
  "required": ["args"],
  "additionalProperties": false
}
```

`args` is the CLI command line (see the [CLI reference](cli.html)); add `"--json"` to get the payloads above. The text result is stdout, then stderr, then `[exit N]` for a non-zero exit; `isError` is `true` for exit `2` only. `--read-only` servers refuse `edit`, `patch`, `rename`, `index` and `--apply`, and report `readOnlyHint: true`.
