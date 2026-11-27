//! Model Context Protocol (MCP) stdio server for Lancet (`lancet mcp`).
//!
//! One tool, `lancet`, whose argument is the command line: `{"args": ["rg",
//! "-w", "spin_lock", "src"]}` runs exactly what `lancet rg -w spin_lock src`
//! runs — the same binary, flags, output and exit codes — in the server's root
//! directory. Every CLI feature is available over MCP the day it ships, and the
//! tool's schema stays a few hundred tokens instead of one schema per command.
//!
//! Newline-delimited JSON-RPC 2.0 over stdin/stdout (protocol revisions
//! `2025-03-26` and `2024-11-05`), no async runtime. `--read-only` refuses the
//! commands that write (`edit`, `patch`, `rename`, `index`, anything `--apply`).

use serde_json::{json, Map, Value};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const DEFAULT_PROTOCOL_VERSION: &str = "2025-03-26";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-03-26", "2024-11-05"];
const DEFAULT_MAX_BYTES: usize = 131_072;
const HARD_MAX_BYTES: usize = 4_194_304;

/// Commands that change files (or the on-disk index).
const WRITING_COMMANDS: &[&str] = &["edit", "write", "patch", "rename", "index"];
/// Commands that accept `--trust-index` and `-j`.
const SEARCHING_COMMANDS: &[&str] = &["rg", "fd", "ast", "refs"];
const SUBCOMMANDS: &[&str] = &[
    "fd", "ast", "rg", "read", "edit", "write", "patch", "rename", "outline", "refs", "index",
    "mcp",
];

const TOOL_DESCRIPTION: &str = "\
Run a lancet command in the workspace. `args` is the command line after `lancet`, \
one array element per argument (no shell: no quoting, globbing, pipes or `&&`). \
Output, flags and exit codes are the CLI's: exit 0 = done, 1 = nothing found (not an \
error), 2 = refused or invalid (message on stderr).\n\
\n\
Orient:   [\"fd\",\"--tree\",\"--dirs\",\"-d\",\"2\",\".\"]  [\"outline\",\"src/server.rs\"]  \
[\"refs\",\"--rank\",\"*Service\",\"--kind\",\"class\",\".\"]\n\
Search:   [\"rg\",\"-w\",\"spin_lock\",\"src\"]  [\"refs\",\"SearchOptions\",\"src\"]  \
[\"ast\",\"$X.unwrap()\",\"src\",\"--lang\",\"rs\"]\n\
Read:     [\"read\",\"src/x.rs\",\"--symbol\",\"handle\"]  [\"read\",\"src/x.rs:120-180\",\"src/y.rs:10-40\"]  \
— the first line is `# PATH rev REV`; keep REV for edits. Large files: add \"--budget-tokens\",\"800\". \
An image comes back as the image (add \"--ocr\" for its text).\n\
Change (preview unless \"--apply\"; pass the rev you read):\n\
  [\"edit\",\"src/x.rs\",\"--old-string\",\"retries = 3\",\"--new-string\",\"retries = 5\",\"--apply\"] \
(fails unless the text occurs once; --replace-all, --rev REV)\n\
  [\"write\",\"src/new.rs\",\"--with-file\",\"-\",\"--apply\"] with `stdin` = the content (an existing file needs --rev)\n\
  [\"edit\",\"--edits\",\"-\",\"--apply\"] with `stdin` = a JSON array of hunks, each with \
\"path\" (+ \"rev\"), one of \"old_string\"/\"lines\"/\"symbol\", and one of \"new_string\"/\"delete\"/\"insert_before\"/\"insert_after\" \
(or {\"path\", \"content\"} for a whole file): \
several files change together or not at all.\n\
  [\"patch\",\"-\",\"--apply\"] with `stdin` = a unified diff or a Codex `*** Begin Patch`.  [\"rename\",\"src/x.rs:42\",\"count\",\"total\",\"--rev\",\"REV\",\"--apply\"] (local variables).\n\
On a conflict (file changed since REV): read again, redo the change, retry with the new rev. \
Add \"--json\" when you need structured output. `lancet --help` / `lancet <cmd> --help` list everything.";

/// Configuration for the Lancet MCP server.
#[derive(Clone, Debug)]
pub struct McpConfig {
    /// Directory commands run in (relative paths in `args` resolve against it).
    pub root: PathBuf,
    pub read_only: bool,
    /// Add `--trust-index` to searches that don't choose an index mode themselves.
    pub trust_index: bool,
    /// Add `-j N` to searches that don't set it themselves.
    pub threads: Option<usize>,
    /// The `lancet` binary to run (defaults to the running executable).
    pub exe: PathBuf,
}

impl McpConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        McpConfig {
            root: root.into(),
            read_only: false,
            trust_index: false,
            threads: None,
            exe: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("lancet")),
        }
    }
}

/// Serve MCP JSON-RPC 2.0 requests on stdin/stdout until EOF.
pub fn serve_stdio(cfg: McpConfig) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve(
        BufReader::new(stdin.lock()),
        BufWriter::new(stdout.lock()),
        cfg,
    )
}

/// Serve MCP JSON-RPC 2.0 requests over arbitrary buffered streams.
pub fn serve<R: BufRead, W: Write>(mut reader: R, mut writer: W, cfg: McpConfig) -> io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(trimmed) {
            Ok(req) => handle_message(&req, &cfg),
            Err(e) => Some(rpc_error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut writer, &reply).map_err(io::Error::other)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

fn handle_message(req: &Value, cfg: &McpConfig) -> Option<Value> {
    let Some(obj) = req.as_object() else {
        return Some(rpc_error(Value::Null, -32600, "invalid request"));
    };
    let id = obj.get("id").cloned();
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return id.map(|req_id| rpc_error(req_id, -32600, "missing string `method`"));
    };
    let params = obj
        .get("params")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    // Notifications carry no `id` and get no response.
    let req_id = id?;
    if method.starts_with("notifications/") {
        return None;
    }
    Some(match method {
        "initialize" => rpc_ok(req_id, handle_initialize(&params, cfg)),
        "ping" => rpc_ok(req_id, json!({})),
        "tools/list" => rpc_ok(req_id, json!({ "tools": [tool_definition(cfg)] })),
        "tools/call" => handle_tools_call(req_id, &params, cfg),
        "prompts/list" => rpc_ok(req_id, json!({ "prompts": [] })),
        _ => rpc_error(req_id, -32601, &format!("method not found: `{method}`")),
    })
}

fn rpc_ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
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
    let mut instructions = String::from(
        "Lancet finds, reads and changes code and documents on one index and one coordinate \
         system. Use the `lancet` tool with the same arguments as the CLI: orient (fd --tree \
         --dirs, outline, refs --rank), locate (rg, refs, ast), read the exact slice and note \
         its rev, then edit/patch/rename with --rev (preview first, --apply to write).",
    );
    if cfg.read_only {
        instructions.push_str(
            " This server is read-only: edit, write, patch, rename, index and --apply are refused.",
        );
    }
    json!({
        "protocolVersion": protocol_version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "lancet", "version": env!("CARGO_PKG_VERSION") },
        "instructions": instructions,
    })
}

fn tool_definition(cfg: &McpConfig) -> Value {
    let mut description = TOOL_DESCRIPTION.to_string();
    if cfg.read_only {
        description.push_str(
            " (Read-only server: edit, write, patch, rename, index and --apply are refused.)",
        );
    }
    json!({
        "name": "lancet",
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "description": "Arguments after `lancet`, one per element, e.g. [\"rg\", \"-w\", \"foo\", \"src\"]"
                },
                "stdin": {
                    "type": "string",
                    "description": "Text for a `-` argument: the hunks for `edit --edits -`, the diff for `patch -`, the text for `--with-file -`"
                },
                "max_output_bytes": {
                    "type": "integer",
                    "minimum": 256,
                    "default": DEFAULT_MAX_BYTES,
                    "description": "Truncate output beyond this many bytes"
                }
            },
            "required": ["args"],
            "additionalProperties": false
        },
        "annotations": {
            "title": "lancet",
            "readOnlyHint": cfg.read_only,
            "destructiveHint": false,
            "idempotentHint": false,
            "openWorldHint": false
        }
    })
}

fn tool_result(content: Vec<Value>, is_error: bool) -> Value {
    json!({ "content": content, "isError": is_error })
}

/// Images a client can display, up to this size each (larger ones stay as their description).
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

/// The image files a `read` command names (without `--ocr`), with their MIME types.
fn read_images(argv: &[String], cfg: &McpConfig) -> Vec<(PathBuf, &'static str)> {
    if subcommand(argv) != Some("read") || argv.iter().any(|a| a == "--ocr") {
        return Vec::new();
    }
    const TAKES_VALUE: &[&str] = &[
        "-l",
        "--lines",
        "--offset",
        "--limit",
        "-s",
        "--symbol",
        "-p",
        "--page",
        "--sheet",
        "--rows",
        "--cols",
        "-e",
        "--entry",
        "--budget-tokens",
    ];
    let mut out = Vec::new();
    let mut it = argv.iter().skip(1);
    while let Some(a) = it.next() {
        if TAKES_VALUE.contains(&a.as_str()) {
            it.next();
            continue;
        }
        if a.starts_with('-') || a.contains('!') {
            continue;
        }
        let path = cfg.root.join(a);
        let mime = crate::read_cmd::image_mime(&path)
            .filter(|m| matches!(*m, "image/png" | "image/jpeg" | "image/gif" | "image/webp"));
        if let Some(m) = mime {
            if std::fs::metadata(&path).is_ok_and(|md| md.is_file() && md.len() <= MAX_IMAGE_BYTES)
            {
                out.push((path, m));
            }
        }
    }
    out
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            s.push(if i <= c.len() {
                T[(n >> shift & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    s
}

fn handle_tools_call(req_id: Value, params: &Map<String, Value>, cfg: &McpConfig) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    if name != "lancet" {
        return rpc_error(
            req_id,
            -32602,
            &format!("unknown tool `{name}` (the only tool is `lancet`)"),
        );
    }
    let args = params
        .get("arguments")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let result = match run_tool(&args, cfg) {
        Ok((content, is_error)) => tool_result(content, is_error),
        Err(msg) => tool_result(
            vec![json!({ "type": "text", "text": format!("error: {msg}") })],
            true,
        ),
    };
    rpc_ok(req_id, result)
}

/// The subcommand an argument list runs (`None` if it isn't one).
fn subcommand(args: &[String]) -> Option<&str> {
    args.first()
        .map(String::as_str)
        .filter(|a| SUBCOMMANDS.contains(a))
}

/// Why `args` is refused on a read-only server, if it is.
fn read_only_refusal(args: &[String]) -> Option<String> {
    let status_only = subcommand(args) == Some("index") && args.iter().any(|a| a == "--status");
    if let Some(cmd) = subcommand(args).filter(|c| WRITING_COMMANDS.contains(c) && !status_only) {
        return Some(format!(
            "`{cmd}` writes files; this MCP server is read-only"
        ));
    }
    args.iter()
        .any(|a| a == "--apply")
        .then(|| "`--apply` writes files; this MCP server is read-only".to_string())
}

fn run_tool(args: &Map<String, Value>, cfg: &McpConfig) -> Result<(Vec<Value>, bool), String> {
    let argv: Vec<String> = match args.get("args") {
        Some(Value::Array(xs)) if !xs.is_empty() => xs
            .iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "every element of `args` must be a string".to_string())
            })
            .collect::<Result<_, _>>()?,
        _ => {
            return Err(
                "`args` must be a non-empty array of strings, e.g. [\"rg\", \"foo\", \"src\"]"
                    .into(),
            )
        }
    };
    if subcommand(&argv) == Some("mcp") {
        return Err("`mcp` can't be run from inside the MCP server".into());
    }
    if cfg.read_only {
        if let Some(why) = read_only_refusal(&argv) {
            return Err(why);
        }
    }
    let stdin = match args.get("stdin") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return Err("`stdin` must be a string".into()),
    };
    let max_bytes = match args.get("max_output_bytes") {
        None | Some(Value::Null) => DEFAULT_MAX_BYTES,
        Some(v) => v
            .as_u64()
            .map(|n| (n as usize).clamp(256, HARD_MAX_BYTES))
            .ok_or("`max_output_bytes` must be a positive integer")?,
    };

    let mut full = argv.clone();
    let searching = subcommand(&argv).is_some_and(|c| SEARCHING_COMMANDS.contains(&c));
    if searching {
        let has = |f: &str| argv.iter().any(|a| a == f);
        if cfg.trust_index && !has("--trust-index") && !has("--no-index") {
            full.push("--trust-index".into());
        }
        if let Some(n) = cfg.threads.filter(|_| !has("-j") && !has("--threads")) {
            full.push("-j".into());
            full.push(n.to_string());
        }
    }

    let mut child = Command::new(&cfg.exe)
        .args(&full)
        .current_dir(&cfg.root)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", cfg.exe.display()))?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        // A separate thread so a command that writes before reading can't deadlock us.
        std::thread::spawn(move || {
            let _ = pipe.write_all(text.as_bytes());
        });
    }
    let mut stderr_pipe = child.stderr.take().unwrap();
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });
    let mut stdout = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_end(&mut stdout);
    }
    let stderr = err_reader.join().unwrap_or_default();
    let status = child.wait().map_err(|e| e.to_string())?;
    let code = status.code().unwrap_or(2);

    let mut text = String::from_utf8_lossy(&stdout).into_owned();
    if text.len() > max_bytes {
        let mut cut = max_bytes;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str(&format!(
            "\n… output truncated at {max_bytes} bytes (narrow the command, or raise max_output_bytes)\n"
        ));
    }
    let err = String::from_utf8_lossy(&stderr);
    if !err.trim().is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(err.trim_end());
        text.push('\n');
    }
    if code != 0 {
        text.push_str(&format!(
            "[exit {code}{}]\n",
            if code == 1 { ": nothing found" } else { "" }
        ));
    }
    // `read` of an image: give the client the image itself, as its own Read would.
    let mut content = vec![json!({ "type": "text", "text": text })];
    for (path, mime) in read_images(&argv, cfg) {
        if let Ok(data) = std::fs::read(&path) {
            content.push(json!({ "type": "image", "data": base64(&data), "mimeType": mime }));
        }
    }
    Ok((content, code >= 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn base64_matches_rfc4648() {
        for (raw, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(raw.as_bytes()), enc);
        }
    }

    #[test]
    fn read_only_refuses_writes_only() {
        for w in [
            &["edit", "a.rs", "--find", "x", "--with", "y"][..],
            &["patch", "-"],
            &["rename", "a.rs:1", "a", "b"],
            &["write", "a.rs", "--content", "x"],
            &["index", "."],
            &["ast", "$X", "--rewrite", "$X", "--apply"],
        ] {
            assert!(read_only_refusal(&v(w)).is_some(), "{w:?}");
        }
        for r in [
            &["rg", "edit"][..],
            &["read", "a.rs"],
            &["ast", "$X", "--rewrite", "$X"],
            &["index", "--status"],
        ] {
            assert!(read_only_refusal(&v(r)).is_none(), "{r:?}");
        }
    }
}
