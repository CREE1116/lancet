---
nav_title: "JSON 스키마 · MCP"
title: "JSON 응답 스키마 & MCP 도구"
description: "Rust 구조체와 일치하는 JSON 출력 스키마, lancet mcp 도구 스키마, JSON-RPC 오류 코드."
lede: "Rust 구조체 기준 기계 판독용 JSON 응답 필드와, CLI 인자를 받는 MCP 도구 하나."
keywords: "json 스키마 응답 mcp inputschema args stdin max_output_bytes json-rpc"
source: "https://github.com/CREE1116/lancet/blob/main/docs/mcp.md"
---
`src/`의 실제 Rust 직렬화 구조체와 `src/mcp.rs`에 정의된 MCP 도구 하나의 스키마입니다.

## Status 열거형 & ErrorInfo {#status}

- `Status`: `"complete"` | `"empty"` | `"invalid_query"` | `"unsupported"` | `"conflict"`
- `ErrorInfo`: `{"message": string, "pos"?: number}` (`pos`는 LQL 파싱 오류 시 설정)

## CLI --json 응답 구조체 {#response-structs}

```bash title="Search · Read · Edit · Rename"
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

// ReadResponse (바이너리 문서·압축 파일에서는 rev 생략; 다중 타깃은 { status, results: [ReadResponse, ...] })
{ "status": "complete", "path": "src/net.rs", "rev": "e70d5a898f122a62", "section": "fn retry_limit", "total_lines": 80, "start_line": 1, "end_line": 4, "returned_lines": 4, "truncated": false, "next_line": null, "estimated_tokens": 28, "lines": [{ "num": 1, "text": "..." }], "elapsed_ms": 0.4 }

// EditResponse (mode: "dry-run" | "applied")
{ "status": "complete", "mode": "applied", "path": "src/net.rs", "rev": "e70d5a898f122a62", "rev_after": "1486bdffbc06c68f", "start_line": 2, "end_line": 2, "hunks": [[2, 2]], "applied": true, "diff": "...", "elapsed_ms": 1.1 }

// RenameResponse (tier: "local-scope")
{ "status": "complete", "mode": "applied", "path": "src/search.rs", "old": "old_n", "new": "new_n", "tier": "local-scope", "rev": "...", "rev_after": "...", "declaration": { "line": 40, "col": 9 }, "sites": [{ "line": 40, "col": 9 }], "comment_mentions": 0, "applied": true, "elapsed_ms": 2.3 }
```

```bash title="Fd · Outline · Refs · Rank · Ast · Patch · Explain"
// FdResponse:      { status, mode, total, returned, truncated, entries: [{ path, kind, file_type?, size?, mtime_ns? }], elapsed_ms }
// OutlineResponse: { status, files: [{ path, file_type, total_lines?, items: [{ kind, name, depth, start_line?, end_line?, signature?, detail? }] }], total_items, truncated, elapsed_ms }
// RefsResponse:    { status, mode, symbol, definitions: [RefHit], references: [RefHit], total_references, truncated, stats }
// RankResponse:    { status, mode, pattern, entries: [{ name, kind, path, line, references, definitions }], total, truncated, files_parsed, elapsed_ms }
// AstResponse:     { status, mode, lang?, pattern, matched, returned, truncated, matches: [AstMatch], diffs: [FileDiff], stats }
// PatchResponse:   { status, mode, applied, files: [{ path, action, from?, hunks, offsets, rev?, rev_after?, diff }], elapsed_ms }
// ExplainResponse: { status, canonical?, ir?, trigram_req?, index_exists, files_considered, index_candidates?, stale_files?, selectivity_pct?, grams_available?, grams_decoded?, elapsed_ms }
```

## MCP: `lancet` 도구 {#mcp-tools}

지원 프로토콜 버전: `2025-03-26`, `2024-11-05`. JSON-RPC 오류 코드: `-32700`(파싱 오류), `-32600`(잘못된 요청), `-32601`(미지원 메서드), `-32602`(없는 도구). 도구는 하나입니다:

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

`args`는 CLI 명령줄입니다([CLI 레퍼런스](cli.html) 참고). 위의 JSON 응답이 필요하면 `"--json"`을 넣으세요. 결과 텍스트는 stdout, stderr, 그리고 종료 코드가 0이 아니면 `[exit N]` 순서이고, `isError`는 종료 코드 `2`일 때만 `true`입니다. `--read-only` 서버는 `edit`, `patch`, `rename`, `index`, `--apply`를 거절하고 `readOnlyHint: true`를 알립니다.
