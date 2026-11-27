//! Model Context Protocol (MCP) stdio server for Lancet (`lancet mcp`).
//!
//! Speaks newline-delimited JSON-RPC 2.0 over stdin/stdout (MCP protocol
//! revisions `2024-11-05` and `2025-03-26`) with zero async runtime or extra
//! protocol dependencies, invoking Lancet's search, AST, read, edit, patch,
//! and index engines directly in-process.

use crate::ast_grep::{search_ast, AstOptions};
use crate::edit_cmd::{edit_file, parse_hunks, EditOptions, Op, Target};
use crate::fd::{find_files, parse_duration, parse_size, FdOptions, PatternMode};
use crate::index::{self, Index};
use crate::outline::{build_outline, OutlineOptions};
use crate::output::Status;
use crate::patch::{apply_patch, PatchOptions};
use crate::query::FileType;
use crate::read_cmd::{parse_col_spec, parse_line_spec, read_file_range, ReadOptions};
use crate::refs::{find_refs, rank_symbols, RefsOptions};
use crate::rename::{rename as rename_local, RenameOptions};
use crate::rg::{parse_filesize, run as run_rg_cmd, RgOptions};
use crate::search::{search, IndexMode, SearchOptions};
use crate::walk::WalkOpts;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

const DEFAULT_PROTOCOL_VERSION: &str = "2025-03-26";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-03-26", "2024-11-05"];
const DEFAULT_RG_MAX_BYTES: usize = 131_072;
const HARD_RG_MAX_BYTES: usize = 4_194_304;

const SERVER_INSTRUCTIONS: &str = "\
Lancet shares one trigram index and one coordinate system across discovery, search, reading, and editing.\n\
Workflow:\n\
1. Orient with `lancet_fd` and `lancet_outline` before reading whole files.\n\
2. Locate targets with `lancet_search`, `lancet_rg`, `lancet_refs`, or `lancet_ast`.\n\
3. Read exact ranges, symbols, PDF pages, spreadsheet cells, or archive entries via `lancet_read` (use `budget_tokens` on large files) and capture `rev`.\n\
4. Change code via `lancet_edit`, `lancet_patch`, or `lancet_ast` (with `rewrite`): defaults to dry-run (`apply: false`). Pass `rev` and `apply: true` to write atomically; on `conflict`, re-read via `lancet_read` and retry with the new `rev`.";

/// Configuration for the Lancet MCP server.
#[derive(Clone, Debug)]
pub struct McpConfig {
    pub root: PathBuf,
    pub read_only: bool,
    pub default_index_mode: IndexMode,
}

impl McpConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        McpConfig {
            root: root.into(),
            read_only: false,
            default_index_mode: IndexMode::Auto,
        }
    }
}

/// Serve MCP JSON-RPC 2.0 requests on stdin/stdout until EOF.
pub fn serve_stdio(cfg: McpConfig) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let reader = BufReader::new(stdin.lock());
    let writer = BufWriter::new(stdout.lock());
    serve(reader, writer, cfg)
}

/// Serve MCP JSON-RPC 2.0 requests over arbitrary buffered streams.
pub fn serve<R: BufRead, W: Write>(reader: R, mut writer: W, cfg: McpConfig) -> io::Result<()> {
    let mut line = String::new();
    let mut r = reader;
    loop {
        line.clear();
        let n = r.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(trimmed) {
            Ok(req) => handle_message(&req, &cfg),
            Err(e) => Some(rpc_error(
                Value::Null,
                -32700,
                &format!("parse error: {e}"),
                None,
            )),
        };
        if let Some(reply) = resp {
            serde_json::to_writer(&mut writer, &reply).map_err(io::Error::other)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

fn handle_message(req: &Value, cfg: &McpConfig) -> Option<Value> {
    let Some(obj) = req.as_object() else {
        return Some(rpc_error(Value::Null, -32600, "invalid request", None));
    };
    let id = obj.get("id").cloned();
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return id.map(|req_id| rpc_error(req_id, -32600, "missing string `method`", None));
    };
    let params = obj
        .get("params")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    // JSON-RPC notifications carry no `id` and must not produce a response.
    let req_id = id?;

    if method.starts_with("notifications/") {
        return None;
    }

    match method {
        "initialize" => Some(rpc_ok(req_id, handle_initialize(&params, cfg))),
        "ping" => Some(rpc_ok(req_id, json!({}))),
        "tools/list" => Some(rpc_ok(req_id, json!({ "tools": tool_definitions(cfg) }))),
        "tools/call" => Some(handle_tools_call(req_id, &params, cfg)),
        "prompts/list" => Some(rpc_ok(req_id, json!({ "prompts": prompt_definitions() }))),
        "prompts/get" => Some(handle_prompts_get(req_id, &params)),
        _ => Some(rpc_error(
            req_id,
            -32601,
            &format!("method not found: `{method}`"),
            None,
        )),
    }
}

fn rpc_ok(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn rpc_error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut err = Map::new();
    err.insert("code".into(), json!(code));
    err.insert("message".into(), json!(message));
    if let Some(d) = data {
        err.insert("data".into(), d);
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": Value::Object(err),
    })
}

fn handle_initialize(params: &Map<String, Value>, cfg: &McpConfig) -> Value {
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PROTOCOL_VERSION);
    let protocol_version = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
        requested
    } else {
        DEFAULT_PROTOCOL_VERSION
    };
    let mut instructions = SERVER_INSTRUCTIONS.to_string();
    if cfg.read_only {
        instructions.push_str(
            "\nNote: this server is running in --read-only mode; write operations are disabled.",
        );
    }
    json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": { "listChanged": false },
            "prompts": { "listChanged": false }
        },
        "serverInfo": {
            "name": "lancet",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": instructions,
    })
}

fn prompt_definitions() -> Value {
    json!([
        {
            "name": "lancet-explore",
            "description": "Orient, search, and read targeted slices of a codebase or document set using Lancet MCP tools.",
            "arguments": [
                {
                    "name": "goal",
                    "description": "What you are looking for in the workspace",
                    "required": false
                }
            ]
        },
        {
            "name": "lancet-safe-edit",
            "description": "Read-then-CAS-edit workflow using `rev` optimistic locking, dry-run previews, and syntax validation.",
            "arguments": [
                {
                    "name": "target",
                    "description": "File path, symbol, or change goal",
                    "required": false
                }
            ]
        }
    ])
}

fn handle_prompts_get(req_id: Value, params: &Map<String, Value>) -> Value {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return rpc_error(req_id, -32602, "missing prompt `name`", None);
    };
    let args = params.get("arguments").and_then(Value::as_object);
    match name {
        "lancet-explore" => {
            let goal = args
                .and_then(|a| a.get("goal"))
                .and_then(Value::as_str)
                .unwrap_or("understand the relevant modules");
            rpc_ok(
                req_id,
                json!({
                    "description": "Orient -> Search -> Read workflow with Lancet",
                    "messages": [{
                        "role": "user",
                        "content": {
                            "type": "text",
                            "text": format!(
                                "Goal: {goal}\n\
                                 1. Run `lancet_fd` (`tree: true`, `max_depth: 2`) and `lancet_outline` to map relevant files.\n\
                                 2. Locate hits with `lancet_search`, `lancet_rg`, `lancet_refs`, or `lancet_ast`.\n\
                                 3. Inspect exact declarations/ranges with `lancet_read` (`symbol`, `lines`, `page`, or `budget_tokens`)."
                            )
                        }
                    }]
                }),
            )
        }
        "lancet-safe-edit" => {
            let target = args
                .and_then(|a| a.get("target"))
                .and_then(Value::as_str)
                .unwrap_or("the target file or symbol");
            rpc_ok(
                req_id,
                json!({
                    "description": "Read-then-CAS-edit workflow with Lancet",
                    "messages": [{
                        "role": "user",
                        "content": {
                            "type": "text",
                            "text": format!(
                                "Target: {target}\n\
                                 1. Call `lancet_read` first to inspect exact lines/symbol and capture `rev`.\n\
                                 2. Preview edits with `lancet_edit` (or `lancet_patch` / `lancet_ast`) leaving `apply: false`.\n\
                                 3. Apply with `rev` and `apply: true`; use `edits` for multiple hunks in one file. If `conflict` occurs, re-read and retry with the new `rev`."
                            )
                        }
                    }]
                }),
            )
        }
        _ => rpc_error(req_id, -32602, &format!("unknown prompt `{name}`"), None),
    }
}

fn rg_tool_def() -> Value {
    let mut props = match json!({
        "pattern": { "type": "string" },
        "path": { "type": "string" },
        "fixed_strings": { "type": "boolean", "default": false },
        "ignore_case": { "type": "boolean", "default": false },
        "smart_case": { "type": "boolean", "default": false },
        "word": { "type": "boolean", "default": false },
        "line_regexp": { "type": "boolean", "default": false },
        "invert": { "type": "boolean", "default": false },
        "multiline": { "type": "boolean", "default": false },
        "multiline_dotall": { "type": "boolean", "default": false },
        "only_matching": { "type": "boolean", "default": false },
        "count": { "type": "boolean", "default": false },
        "files_with_matches": { "type": "boolean", "default": false },
        "max_count": { "type": "integer", "minimum": 1 },
        "before": { "type": "integer", "minimum": 0 },
        "after": { "type": "integer", "minimum": 0 }
    }) {
        Value::Object(m) => m,
        _ => unreachable!(),
    };
    if let Value::Object(extra) = json!({
        "context": { "type": "integer", "minimum": 0 },
        "globs": { "type": "array", "items": { "type": "string" } },
        "iglobs": { "type": "array", "items": { "type": "string" } },
        "types": { "type": "array", "items": { "type": "string" } },
        "types_not": { "type": "array", "items": { "type": "string" } },
        "type_add": { "type": "array", "items": { "type": "string" }, "description": "Custom ripgrep type definitions (`NAME:GLOB`)" },
        "max_depth": { "type": "integer", "minimum": 0 },
        "max_filesize": { "type": "string", "description": "e.g. `500K`, `2M`" },
        "follow": { "type": "boolean", "default": false },
        "docs": { "type": "boolean", "default": false },
        "passthru": { "type": "boolean", "default": false },
        "index_mode": { "type": "string", "enum": ["auto", "off", "trust"] },
        "hidden": { "type": "boolean", "default": false },
        "no_ignore": { "type": "boolean", "default": false },
        "max_output_bytes": { "type": "integer", "minimum": 256, "default": DEFAULT_RG_MAX_BYTES }
    }) {
        props.extend(extra);
    }
    json!({
        "name": "lancet_rg",
        "description": "ripgrep-compatible regex/fixed-string search on Lancet's always-fresh trigram index and optional in-process document/archive extractors (`docs: true`). Returns deterministic path-sorted lines with line numbers.",
        "annotations": {
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        },
        "inputSchema": {
            "type": "object",
            "required": ["pattern"],
            "properties": Value::Object(props)
        }
    })
}

fn tool_definitions(cfg: &McpConfig) -> Vec<Value> {
    let ro_note = if cfg.read_only {
        " (server is --read-only: `apply: true` is disabled)"
    } else {
        ""
    };
    let mut tools = vec![
        json!({
            "name": "lancet_fd",
            "description": "Fast .gitignore-aware file and folder explorer with optional tree rendering, extension/type/size/recency filters, and `--trust-index` zero-walk lookups.",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Root path relative to server root (default `.`)" },
                    "pattern": { "type": "string", "description": "Optional filename/path match pattern" },
                    "pattern_mode": { "type": "string", "enum": ["substring", "glob", "regex"], "default": "substring" },
                    "case_sensitive": { "type": "boolean", "default": false },
                    "kinds": { "type": "array", "items": { "type": "string", "enum": ["file", "dir"] } },
                    "dirs": { "type": "boolean", "default": false, "description": "Only directories (same as kinds: [\"dir\"])" },
                    "exts": { "type": "array", "items": { "type": "string" } },
                    "type_classes": { "type": "array", "items": { "type": "string" } },
                    "max_depth": { "type": "integer", "minimum": 0 },
                    "changed_within": { "type": "string", "description": "e.g. `30m`, `2h`, `7d`" },
                    "min_size": { "type": "string", "description": "e.g. `1k`, `5m`" },
                    "max_size": { "type": "string", "description": "e.g. `10m`, `1g`" },
                    "tree": { "type": "boolean", "default": false },
                    "limit": { "type": "integer", "minimum": 1 },
                    "index_mode": { "type": "string", "enum": ["auto", "off", "trust"] },
                    "hidden": { "type": "boolean", "default": false },
                    "no_ignore": { "type": "boolean", "default": false },
                    "meta": { "type": "boolean", "default": false }
                }
            }
        }),
        json!({
            "name": "lancet_search",
            "description": "Search code, PDFs, Office documents, HWP/HWPX, and archives using Lancet's query DSL (AND/OR/NOT, near/before, fuzzy, regex, in line/block/file/function, pipes like `| expand parent | limit 10 | budget tokens:1500`).",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string" },
                    "path": { "type": "string" },
                    "max_lines": { "type": "integer", "minimum": 1, "default": 20 },
                    "max_cols": { "type": "integer", "minimum": 1, "default": 240 },
                    "index_mode": { "type": "string", "enum": ["auto", "off", "trust"] },
                    "hidden": { "type": "boolean", "default": false },
                    "no_ignore": { "type": "boolean", "default": false },
                    "ocr": { "type": "boolean", "default": false }
                }
            }
        }),
        rg_tool_def(),
        json!({
            "name": "lancet_outline",
            "description": "Extract Tree-sitter declaration skeletons (functions, structs, classes), Markdown headings, spreadsheet schemas, or archive entry manifests.",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max_depth": { "type": "integer", "minimum": 0 },
                    "exts": { "type": "array", "items": { "type": "string" } },
                    "limit": { "type": "integer", "minimum": 1 },
                    "hidden": { "type": "boolean", "default": false },
                    "no_ignore": { "type": "boolean", "default": false },
                    "ocr": { "type": "boolean", "default": false }
                }
            }
        }),
        json!({
            "name": "lancet_read",
            "description": "Surgically read exact line ranges, AST symbols (including doc comments/attributes), PDF/PPTX pages, spreadsheet sheets/rows/cols, or archive entries (`archive.zip!entry`) on the exact same line coordinate system as search, returning `rev` (XXH3-64) for CAS-safe edits.",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "required": ["target"],
                "properties": {
                    "target": { "type": "string", "description": "File path (`file.rs`, `file.rs:10-50`, or `archive.zip!entry`)" },
                    "lines": { "type": "string", "description": "1-based line range (`10:50`, `42`, `100:`)" },
                    "symbol": { "type": "string", "description": "AST declaration name (`fn`, `struct`, `class`, etc.)" },
                    "page": { "type": "string", "description": "PDF page or PPTX slide range (`2` or `2:5`)" },
                    "sheet": { "type": "string" },
                    "rows": { "type": "string" },
                    "cols": { "type": "string" },
                    "entry": { "type": "string" },
                    "budget_tokens": { "type": "integer", "minimum": 1 },
                    "ocr": { "type": "boolean", "default": false }
                }
            }
        }),
        json!({
            "name": "lancet_refs",
            "description": "Index-accelerated `[def]` declarations and lexical AST `[ref]` occurrences across 24 languages (comments and string literals excluded). Set `rank: true` to treat `symbol` as a declaration glob (e.g. `*Service`) and rank matching declarations by reference count.",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "required": ["symbol"],
                "properties": {
                    "symbol": { "type": "string", "description": "Exact symbol name (or glob pattern when `rank: true`, e.g. `*Service`)" },
                    "path": { "type": "string" },
                    "lang": { "type": "string" },
                    "exts": { "type": "array", "items": { "type": "string" } },
                    "paths": { "type": "array", "items": { "type": "string" } },
                    "all_files": { "type": "boolean", "default": false },
                    "rank": { "type": "boolean", "default": false, "description": "Rank declarations matching `symbol` glob by lexical reference count" },
                    "kinds": { "type": "array", "items": { "type": "string" }, "description": "Filter declaration kinds when `rank: true` (e.g. `class`, `function`, `struct`)" },
                    "limit": { "type": "integer", "minimum": 1 },
                    "index_mode": { "type": "string", "enum": ["auto", "off", "trust"] },
                    "hidden": { "type": "boolean", "default": false },
                    "no_ignore": { "type": "boolean", "default": false }
                }
            }
        }),
        json!({
            "name": "lancet_ast",
            "description": format!("Tree-sitter structural AST pattern search and optional rewrite across 24 languages (`$VAR`, `$_`, `$$$ARGS`, plus statement metavariables). Rewrites default to dry-run (`apply: false`) unless `apply: true`.{ro_note}"),
            "annotations": {
                "readOnlyHint": cfg.read_only,
                "destructiveHint": !cfg.read_only,
                "idempotentHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "required": ["pattern"],
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "lang": { "type": "string" },
                    "rewrite": { "type": "string" },
                    "apply": { "type": "boolean", "default": false },
                    "paths": { "type": "array", "items": { "type": "string" } },
                    "limit": { "type": "integer", "minimum": 1 },
                    "index_mode": { "type": "string", "enum": ["auto", "off", "trust"] },
                    "hidden": { "type": "boolean", "default": false },
                    "no_ignore": { "type": "boolean", "default": false }
                }
            }
        }),
    ];

    if !cfg.read_only {
        tools.push(json!({
            "name": "lancet_edit",
            "description": "Replace, delete, or insert text addressed by `lines`, `symbol`, `find` (unique exact text, or `all: true`), or atomic multi-hunk `edits`. Dry-run by default (`apply: false`); pass `rev` from `lancet_read` and `apply: true` for locked compare-and-swap writes with Tree-sitter syntax verification.",
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": true,
                "idempotentHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": { "type": "string" },
                    "lines": { "type": "string", "description": "1-based range (`10:50`, `42`, `100:`)" },
                    "symbol": { "type": "string" },
                    "find": { "type": "string", "description": "Exact text to target (unique unless `all: true`)" },
                    "all": { "type": "boolean", "default": false },
                    "with": { "type": "string", "description": "Replacement text" },
                    "delete": { "type": "boolean", "default": false },
                    "insert_before": { "type": "string" },
                    "insert_after": { "type": "string" },
                    "edits": {
                        "type": "array",
                        "description": "Multiple hunks applied atomically in original coordinates",
                        "items": {
                            "type": "object",
                            "properties": {
                                "lines": { "type": "string" },
                                "symbol": { "type": "string" },
                                "find": { "type": "string" },
                                "all": { "type": "boolean" },
                                "with": { "type": "string" },
                                "delete": { "type": "boolean" },
                                "insert_before": { "type": "string" },
                                "insert_after": { "type": "string" }
                            }
                        }
                    },
                    "rev": { "type": "string", "description": "Expected file revision from `lancet_read` / prior `lancet_edit`" },
                    "apply": { "type": "boolean", "default": false },
                    "allow_syntax_errors": { "type": "boolean", "default": false }
                }
            }
        }));
        tools.push(json!({
            "name": "lancet_patch",
            "description": "Apply a unified diff (`git diff` / `diff -u`) across multiple files with exact hunk verification, all-or-nothing pre-check, locked CAS writes with rollback, and syntax-error refusal. Dry-run by default unless `apply: true`.",
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": true,
                "idempotentHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "required": ["patch"],
                "properties": {
                    "patch": { "type": "string" },
                    "root": { "type": "string", "description": "Directory for patch paths (default server root)" },
                    "strip": { "type": "integer", "minimum": 0, "default": 1 },
                    "apply": { "type": "boolean", "default": false },
                    "allow_syntax_errors": { "type": "boolean", "default": false }
                }
            }
        }));
        tools.push(json!({
            "name": "lancet_rename",
            "description": "Tier-1 local-scope symbol rename in one file (Rust, Python, JS/TS/TSX, Go) picked by position (`path`, `line`, optional `col`, `old`, `new`). Uses per-language scope resolution with shadowing/capture checks and locked CAS writes; refuses non-local or opaque constructs rather than guessing. Dry-run by default unless `apply: true`.",
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": true,
                "idempotentHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "required": ["path", "line", "old", "new"],
                "properties": {
                    "path": { "type": "string" },
                    "line": { "type": "integer", "minimum": 1, "description": "1-based line of a declaration or use" },
                    "col": { "type": "integer", "minimum": 1, "description": "Optional 1-based byte column when the line has multiple bindings named `old`" },
                    "old": { "type": "string" },
                    "new": { "type": "string" },
                    "rev": { "type": "string", "description": "Expected file revision from `lancet_read`" },
                    "apply": { "type": "boolean", "default": false }
                }
            }
        }));
    }

    tools.push(json!({
        "name": "lancet_index",
        "description": format!("Inspect, incrementally sync, rebuild, or compact the `.lancet` trigram index.{ro_note}"),
        "annotations": {
            "readOnlyHint": cfg.read_only,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        },
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["status", "sync", "rebuild", "compact"], "default": "status" },
                "path": { "type": "string" },
                "hidden": { "type": "boolean", "default": false },
                "no_ignore": { "type": "boolean", "default": false },
                "ocr": { "type": "boolean", "default": false }
            }
        }
    }));

    tools
}

fn handle_tools_call(req_id: Value, params: &Map<String, Value>, cfg: &McpConfig) -> Value {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return rpc_error(req_id, -32602, "missing tool `name`", None);
    };
    let args = match params.get("arguments") {
        None => Map::new(),
        Some(Value::Object(m)) => m.clone(),
        Some(_) => return rpc_error(req_id, -32602, "`arguments` must be a JSON object", None),
    };

    let res = match name {
        "lancet_fd" => call_fd(&args, cfg),
        "lancet_search" => call_search(&args, cfg),
        "lancet_rg" => call_rg(&args, cfg),
        "lancet_outline" => call_outline(&args, cfg),
        "lancet_read" => call_read(&args, cfg),
        "lancet_refs" => call_refs(&args, cfg),
        "lancet_ast" => call_ast(&args, cfg),
        "lancet_edit" => call_edit(&args, cfg),
        "lancet_patch" => call_patch(&args, cfg),
        "lancet_rename" => call_rename(&args, cfg),
        "lancet_index" => call_index(&args, cfg),
        _ => return rpc_error(req_id, -32602, &format!("unknown tool `{name}`"), None),
    };

    match res {
        Ok((payload, is_error)) => rpc_ok(req_id, tool_result_value(&payload, is_error)),
        Err(msg) => rpc_ok(req_id, tool_error_value(&msg)),
    }
}

fn tool_result_value(payload: &Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    })
}

fn tool_error_value(message: &str) -> Value {
    tool_result_value(
        &json!({ "status": "invalid_query", "error": { "message": message } }),
        true,
    )
}

fn to_tool_payload<T: Serialize>(resp: &T, status: Status) -> Result<(Value, bool), String> {
    let val = serde_json::to_value(resp).map_err(|e| format!("serialization error: {e}"))?;
    let is_err = matches!(
        status,
        Status::InvalidQuery | Status::Unsupported | Status::Conflict
    );
    Ok((val, is_err))
}

fn opt_str<'a>(args: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(format!("argument `{key}` must be a string")),
    }
}

fn req_str<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    opt_str(args, key)?.ok_or_else(|| format!("missing required argument `{key}`"))
}

fn opt_bool(args: &Map<String, Value>, key: &str, default: bool) -> Result<bool, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(format!("argument `{key}` must be a boolean")),
    }
}

fn opt_usize(args: &Map<String, Value>, key: &str) -> Result<Option<usize>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .map(Some)
            .ok_or_else(|| format!("argument `{key}` must be a non-negative integer")),
        Some(_) => Err(format!("argument `{key}` must be a non-negative integer")),
    }
}

fn opt_u64(args: &Map<String, Value>, key: &str) -> Result<Option<u64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("argument `{key}` must be a non-negative integer")),
        Some(_) => Err(format!("argument `{key}` must be a non-negative integer")),
    }
}

fn opt_str_vec(args: &Map<String, Value>, key: &str) -> Result<Vec<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(arr)) => arr
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("argument `{key}` elements must be strings"))
            })
            .collect(),
        Some(_) => Err(format!("argument `{key}` must be an array of strings")),
    }
}

fn parse_index_mode(
    args: &Map<String, Value>,
    default_mode: IndexMode,
) -> Result<IndexMode, String> {
    match opt_str(args, "index_mode")? {
        None => Ok(default_mode),
        Some("auto") => Ok(IndexMode::Auto),
        Some("off") => Ok(IndexMode::Off),
        Some("trust") => Ok(IndexMode::Trust),
        Some(other) => Err(format!(
            "invalid `index_mode` `{other}` (expected `auto`, `off`, or `trust`)"
        )),
    }
}

fn resolve_path(root: &Path, raw: Option<&str>) -> PathBuf {
    match raw.map(str::trim).filter(|s| !s.is_empty() && *s != ".") {
        None => root.to_path_buf(),
        Some(s) => {
            let p = Path::new(s);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                root.join(p)
            }
        }
    }
}

fn resolve_read_target(root: &Path, raw: &str) -> String {
    if let Some((left, right)) = raw.split_once('!') {
        let lp = Path::new(left);
        if lp.is_absolute() {
            raw.to_string()
        } else {
            format!("{}!{right}", root.join(lp).display())
        }
    } else {
        let p = Path::new(raw);
        if p.is_absolute() {
            raw.to_string()
        } else {
            root.join(p).display().to_string()
        }
    }
}

fn call_fd(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    let root = resolve_path(&cfg.root, opt_str(args, "path")?);
    let mut opts = FdOptions::new(root);
    opts.pattern = opt_str(args, "pattern")?.map(str::to_string);
    opts.pattern_mode = match opt_str(args, "pattern_mode")? {
        None | Some("substring") => PatternMode::Substring,
        Some("glob") => PatternMode::Glob,
        Some("regex") => PatternMode::Regex,
        Some(other) => {
            return Err(format!(
                "invalid `pattern_mode` `{other}` (expected `substring`, `glob`, or `regex`)"
            ))
        }
    };
    opts.case_sensitive = opt_bool(args, "case_sensitive", false)?;
    for k in opt_str_vec(args, "kinds")? {
        match k.to_ascii_lowercase().as_str() {
            "f" | "file" => opts.include_files = true,
            "d" | "dir" | "directory" => opts.include_dirs = true,
            other => return Err(format!("unknown kind `{other}` (expected `file` or `dir`)")),
        }
    }
    if opt_bool(args, "dirs", false)? {
        opts.include_dirs = true;
    }
    opts.exts = opt_str_vec(args, "exts")?;
    for tc in opt_str_vec(args, "type_classes")? {
        let ft = FileType::parse(&tc).ok_or_else(|| format!("unknown type_class `{tc}`"))?;
        opts.file_types.push(ft);
    }
    opts.max_depth = opt_usize(args, "max_depth")?;
    opts.changed_within_ns = opt_str(args, "changed_within")?
        .map(parse_duration)
        .transpose()?;
    opts.min_size = opt_str(args, "min_size")?.map(parse_size).transpose()?;
    opts.max_size = opt_str(args, "max_size")?.map(parse_size).transpose()?;
    opts.tree = opt_bool(args, "tree", false)?;
    opts.limit = opt_usize(args, "limit")?;
    opts.index = parse_index_mode(args, cfg.default_index_mode)?;
    opts.need_meta = opt_bool(args, "meta", false)?;
    opts.walk = WalkOpts {
        hidden: opt_bool(args, "hidden", false)?,
        no_ignore: opt_bool(args, "no_ignore", false)?,
        ..WalkOpts::default()
    };
    let resp = find_files(&opts);
    to_tool_payload(&resp, resp.status)
}

fn call_search(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    let query = req_str(args, "query")?;
    let root = resolve_path(&cfg.root, opt_str(args, "path")?);
    let mut opts = SearchOptions::new(root);
    opts.index = parse_index_mode(args, cfg.default_index_mode)?;
    opts.max_lines = opt_usize(args, "max_lines")?.unwrap_or(20);
    opts.max_cols = opt_usize(args, "max_cols")?.unwrap_or(240);
    opts.ocr = opt_bool(args, "ocr", false)?;
    opts.walk = WalkOpts {
        hidden: opt_bool(args, "hidden", false)?,
        no_ignore: opt_bool(args, "no_ignore", false)?,
        ..WalkOpts::default()
    };
    let resp = search(query, &opts);
    to_tool_payload(&resp, resp.status)
}

fn call_rg(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    let pattern = req_str(args, "pattern")?.to_string();
    let path = resolve_path(&cfg.root, opt_str(args, "path")?);
    let mut o = RgOptions::new(vec![pattern]);
    o.paths = vec![path];
    o.fixed_strings = opt_bool(args, "fixed_strings", false)?;
    o.case = if opt_bool(args, "ignore_case", false)? {
        crate::rg::Case::Insensitive
    } else if opt_bool(args, "smart_case", false)? {
        crate::rg::Case::Smart
    } else {
        crate::rg::Case::Sensitive
    };
    o.word = opt_bool(args, "word", false)?;
    o.line_regexp = opt_bool(args, "line_regexp", false)?;
    o.invert = opt_bool(args, "invert", false)?;
    o.multiline = opt_bool(args, "multiline", false)?;
    o.multiline_dotall = opt_bool(args, "multiline_dotall", false)?;
    o.only_matching = opt_bool(args, "only_matching", false)?;
    o.count = opt_bool(args, "count", false)?;
    o.files_with_matches = opt_bool(args, "files_with_matches", false)?;
    o.max_count = opt_u64(args, "max_count")?;
    let ctx = opt_usize(args, "context")?.unwrap_or(0);
    o.before = opt_usize(args, "before")?.unwrap_or(ctx);
    o.after = opt_usize(args, "after")?.unwrap_or(ctx);
    o.line_number = true;
    o.color = false;
    o.docs = opt_bool(args, "docs", false)?;
    o.passthru = opt_bool(args, "passthru", false)?;
    o.globs = opt_str_vec(args, "globs")?;
    o.iglobs = opt_str_vec(args, "iglobs")?;
    o.types = opt_str_vec(args, "types")?;
    o.types_not = opt_str_vec(args, "types_not")?;
    o.type_add = opt_str_vec(args, "type_add")?;
    o.index = parse_index_mode(args, cfg.default_index_mode)?;
    let max_filesize = opt_str(args, "max_filesize")?
        .map(parse_filesize)
        .transpose()?;
    o.walk = WalkOpts {
        hidden: opt_bool(args, "hidden", false)?,
        no_ignore: opt_bool(args, "no_ignore", false)?,
        follow: opt_bool(args, "follow", false)?,
        max_depth: opt_usize(args, "max_depth")?,
        max_filesize,
        ..WalkOpts::default()
    };

    let max_bytes = opt_usize(args, "max_output_bytes")?
        .unwrap_or(DEFAULT_RG_MAX_BYTES)
        .clamp(256, HARD_RG_MAX_BYTES);

    let mut buf = Vec::new();
    let matched = run_rg_cmd(&o, &mut buf)?;
    let full = String::from_utf8_lossy(&buf);
    let total_bytes = full.len();
    let truncated = total_bytes > max_bytes;
    let output = if truncated {
        let mut end = max_bytes;
        while end > 0 && !full.is_char_boundary(end) {
            end -= 1;
        }
        if let Some(nl) = full[..end].rfind('\n') {
            if nl > 0 {
                end = nl + 1;
            }
        }
        full[..end].to_string()
    } else {
        full.into_owned()
    };
    Ok((
        json!({
            "status": if matched { "complete" } else { "empty" },
            "matched": matched,
            "truncated": truncated,
            "total_bytes": total_bytes,
            "output": output,
        }),
        false,
    ))
}

fn call_outline(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    let root = resolve_path(&cfg.root, opt_str(args, "path")?);
    let mut opts = OutlineOptions::new(root);
    opts.max_depth = opt_usize(args, "max_depth")?;
    opts.exts = opt_str_vec(args, "exts")?;
    opts.limit = opt_usize(args, "limit")?;
    opts.ocr = opt_bool(args, "ocr", false)?;
    opts.walk = WalkOpts {
        hidden: opt_bool(args, "hidden", false)?,
        no_ignore: opt_bool(args, "no_ignore", false)?,
        ..WalkOpts::default()
    };
    let resp = build_outline(&opts);
    to_tool_payload(&resp, resp.status)
}

fn call_read(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    let raw_target = req_str(args, "target")?;
    let mut opts = ReadOptions::new(resolve_read_target(&cfg.root, raw_target));
    opts.lines = opt_str(args, "lines")?.map(parse_line_spec).transpose()?;
    opts.symbol = opt_str(args, "symbol")?.map(str::to_string);
    opts.pages = match opt_str(args, "page")? {
        Some(raw) => {
            let (s, e) = parse_line_spec(raw)?;
            Some((s, e.unwrap_or(s)))
        }
        None => None,
    };
    opts.sheet = opt_str(args, "sheet")?.map(str::to_string);
    opts.rows = opt_str(args, "rows")?.map(parse_line_spec).transpose()?;
    opts.cols = opt_str(args, "cols")?.map(parse_col_spec).transpose()?;
    opts.entry = opt_str(args, "entry")?.map(str::to_string);
    opts.budget_tokens = opt_usize(args, "budget_tokens")?;
    opts.ocr = opt_bool(args, "ocr", false)?;
    let resp = read_file_range(&opts);
    to_tool_payload(&resp, resp.status)
}

fn call_refs(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    let rank = opt_bool(args, "rank", false)?;
    let kinds = opt_str_vec(args, "kinds")?;
    if !kinds.is_empty() && !rank {
        return Err("`kinds` only applies when `rank: true`".into());
    }
    let symbol = if rank {
        opt_str(args, "symbol")?.unwrap_or("*")
    } else {
        req_str(args, "symbol")?
    };
    let root = resolve_path(&cfg.root, opt_str(args, "path")?);
    let mut opts = RefsOptions::new(symbol, root);
    opts.lang = opt_str(args, "lang")?.map(str::to_string);
    opts.exts = opt_str_vec(args, "exts")?;
    opts.paths = opt_str_vec(args, "paths")?;
    opts.all_files = opt_bool(args, "all_files", false)?;
    opts.limit = opt_usize(args, "limit")?;
    opts.index = parse_index_mode(args, cfg.default_index_mode)?;
    opts.walk = WalkOpts {
        hidden: opt_bool(args, "hidden", false)?,
        no_ignore: opt_bool(args, "no_ignore", false)?,
        ..WalkOpts::default()
    };
    if rank {
        let resp = rank_symbols(&opts, &kinds);
        to_tool_payload(&resp, resp.status)
    } else {
        let resp = find_refs(&opts);
        to_tool_payload(&resp, resp.status)
    }
}

fn call_ast(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    let pattern = req_str(args, "pattern")?;
    let apply = opt_bool(args, "apply", false)?;
    if apply && cfg.read_only {
        return Err("server is running in --read-only mode; `apply: true` is disabled".into());
    }
    let root = resolve_path(&cfg.root, opt_str(args, "path")?);
    let mut opts = AstOptions::new(pattern, root);
    opts.lang = opt_str(args, "lang")?.map(str::to_string);
    opts.rewrite = opt_str(args, "rewrite")?.map(str::to_string);
    opts.apply = apply;
    opts.paths = opt_str_vec(args, "paths")?;
    opts.limit = opt_usize(args, "limit")?;
    opts.index = parse_index_mode(args, cfg.default_index_mode)?;
    opts.walk = WalkOpts {
        hidden: opt_bool(args, "hidden", false)?,
        no_ignore: opt_bool(args, "no_ignore", false)?,
        ..WalkOpts::default()
    };
    let resp = search_ast(&opts);
    to_tool_payload(&resp, resp.status)
}

fn call_edit(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    if cfg.read_only {
        return Err("server is running in --read-only mode; `lancet_edit` is disabled".into());
    }
    let path = resolve_path(&cfg.root, Some(req_str(args, "path")?));
    let rev = opt_str(args, "rev")?.map(str::to_string);
    let apply = opt_bool(args, "apply", false)?;
    let allow_syntax_errors = opt_bool(args, "allow_syntax_errors", false)?;

    let mut opts = if let Some(edits_val) = args.get("edits").filter(|v| !v.is_null()) {
        let json_str =
            serde_json::to_string(edits_val).map_err(|e| format!("invalid `edits`: {e}"))?;
        let hunks = parse_hunks(&json_str)?;
        EditOptions::multi(path, hunks)
    } else {
        let lines = opt_str(args, "lines")?;
        let symbol = opt_str(args, "symbol")?;
        let find = opt_str(args, "find")?;
        let all = opt_bool(args, "all", false)?;
        if all && find.is_none() {
            return Err("`all` only applies to `find`".into());
        }
        let target = match (lines, symbol, find) {
            (Some(l), None, None) => {
                let (a, b) = parse_line_spec(l)?;
                Target::Lines(a, b)
            }
            (None, Some(s), None) => Target::Symbol(s.to_string()),
            (None, None, Some(t)) => Target::Find {
                text: t.to_string(),
                all,
            },
            _ => return Err("give `edits` or exactly one of `lines`, `symbol`, or `find`".into()),
        };

        let with = opt_str(args, "with")?;
        let delete = opt_bool(args, "delete", false)?;
        let insert_before = opt_str(args, "insert_before")?;
        let insert_after = opt_str(args, "insert_after")?;
        let ops: Vec<Op> = [
            with.map(|s| Op::Replace(s.to_string())),
            delete.then_some(Op::Delete),
            insert_before.map(|s| Op::InsertBefore(s.to_string())),
            insert_after.map(|s| Op::InsertAfter(s.to_string())),
        ]
        .into_iter()
        .flatten()
        .collect();
        if ops.len() != 1 {
            return Err(
                "give exactly one of `with`, `delete: true`, `insert_before`, or `insert_after`"
                    .into(),
            );
        }
        EditOptions::new(path, target, ops.into_iter().next().unwrap())
    };
    opts.rev = rev;
    opts.apply = apply;
    opts.allow_syntax_errors = allow_syntax_errors;

    let resp = edit_file(&opts);
    to_tool_payload(&resp, resp.status)
}

fn call_patch(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    if cfg.read_only {
        return Err("server is running in --read-only mode; `lancet_patch` is disabled".into());
    }
    let patch_text = req_str(args, "patch")?;
    let root = resolve_path(&cfg.root, opt_str(args, "root")?);
    let mut opts = PatchOptions::new(patch_text, root);
    opts.strip = opt_usize(args, "strip")?.unwrap_or(1);
    opts.apply = opt_bool(args, "apply", false)?;
    opts.allow_syntax_errors = opt_bool(args, "allow_syntax_errors", false)?;
    let resp = apply_patch(&opts);
    to_tool_payload(&resp, resp.status)
}

fn call_rename(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    if cfg.read_only {
        return Err("server is running in --read-only mode; `lancet_rename` is disabled".into());
    }
    let path = resolve_path(&cfg.root, Some(req_str(args, "path")?));
    let line =
        opt_usize(args, "line")?.ok_or_else(|| "missing required argument `line`".to_string())?;
    if line == 0 {
        return Err("`line` must be >= 1 (1-based)".into());
    }
    let col = opt_usize(args, "col")?;
    if col == Some(0) {
        return Err("`col` must be >= 1 (1-based)".into());
    }
    let old = req_str(args, "old")?.to_string();
    let new = req_str(args, "new")?.to_string();
    let rev = opt_str(args, "rev")?.map(str::to_string);
    let apply = opt_bool(args, "apply", false)?;

    let opts = RenameOptions {
        path,
        line,
        col,
        old,
        new,
        rev,
        apply,
    };
    let resp = rename_local(&opts);
    to_tool_payload(&resp, resp.status)
}

fn call_index(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Value, bool), String> {
    let action = opt_str(args, "action")?.unwrap_or("status");
    if cfg.read_only && action != "status" {
        return Err(format!(
            "server is running in --read-only mode; `action: {action}` is disabled"
        ));
    }
    let root = resolve_path(&cfg.root, opt_str(args, "path")?);
    let dir = index::default_dir(&root);
    let wopts = WalkOpts {
        hidden: opt_bool(args, "hidden", false)?,
        no_ignore: opt_bool(args, "no_ignore", false)?,
        ..WalkOpts::default()
    };
    let ocr = opt_bool(args, "ocr", false)?;

    match action {
        "status" => match Index::open(&dir).map_err(|e| format!("{e:#}"))? {
            Some(ix) => {
                let bytes: usize = ix.segs.iter().map(|s| s.disk_bytes()).sum();
                Ok((
                    json!({
                        "status": "complete",
                        "exists": true,
                        "index_dir": dir,
                        "segments": ix.segs.len(),
                        "live_docs": ix.live_count(),
                        "dead_docs": ix.dead_count(),
                        "disk_bytes": bytes,
                    }),
                    false,
                ))
            }
            None => Ok((
                json!({
                    "status": "empty",
                    "exists": false,
                    "index_dir": dir,
                }),
                false,
            )),
        },
        "sync" => {
            let rep =
                index::sync_with_ocr(&root, &dir, &wopts, ocr).map_err(|e| format!("{e:#}"))?;
            Ok((json!({ "status": "complete", "report": rep }), false))
        }
        "rebuild" => {
            let rep =
                index::rebuild_with_ocr(&root, &dir, &wopts, ocr).map_err(|e| format!("{e:#}"))?;
            Ok((json!({ "status": "complete", "report": rep }), false))
        }
        "compact" => {
            let mut rep =
                index::sync_with_ocr(&root, &dir, &wopts, ocr).map_err(|e| format!("{e:#}"))?;
            let compacted = index::compact(&dir).map_err(|e| format!("{e:#}"))?;
            rep.compacted += compacted;
            if let Ok(Some(ix)) = Index::open(&dir) {
                rep.segments = ix.segs.len();
            }
            Ok((json!({ "status": "complete", "report": rep }), false))
        }
        other => Err(format!(
            "invalid `action` `{other}` (expected `status`, `sync`, `rebuild`, or `compact`)"
        )),
    }
}
