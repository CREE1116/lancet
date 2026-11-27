use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Cursor, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use tempfile::TempDir;

struct McpProc {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl McpProc {
    fn spawn(root: &Path, extra_args: &[&str]) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lancet"));
        cmd.arg("mcp").arg("--root").arg(root);
        for a in extra_args {
            cmd.arg(a);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn lancet mcp");
        let stdin = BufWriter::new(child.stdin.take().unwrap());
        let stdout = BufReader::new(child.stdout.take().unwrap());
        McpProc {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        serde_json::to_writer(&mut self.stdin, &msg).unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();

        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).unwrap();
        assert!(n > 0, "MCP server closed stdout unexpectedly");
        let reply: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(reply["id"], json!(id));
        reply
    }

    fn notify(&mut self, method: &str, params: Value) {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        serde_json::to_writer(&mut self.stdin, &msg).unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Call the `lancet` tool; returns (text, isError).
    fn run(&mut self, args: &[&str], stdin: Option<&str>) -> (String, bool) {
        let mut a = json!({ "args": args });
        if let Some(s) = stdin {
            a["stdin"] = json!(s);
        }
        let reply = self.request("tools/call", json!({ "name": "lancet", "arguments": a }));
        assert!(
            reply.get("error").is_none(),
            "unexpected RPC error: {reply}"
        );
        let res = &reply["result"];
        (
            res["content"][0]["text"].as_str().unwrap().to_string(),
            res["isError"].as_bool().unwrap(),
        )
    }

    fn rev(&mut self, path: &str) -> String {
        let (text, err) = self.run(&["read", path, "--json"], None);
        assert!(!err, "{text}");
        let v: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        v["rev"].as_str().unwrap().to_string()
    }
}

impl Drop for McpProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn mcp_protocol_handshake_errors_and_notifications() {
    let dir = TempDir::new().unwrap();
    let mut cfg = lancet::mcp::McpConfig::new(dir.path());
    cfg.exe = env!("CARGO_BIN_EXE_lancet").into();
    let input = concat!(
        "{invalid json\n",
        "\n",
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/list"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"lancet_rg","arguments":{}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"lancet","arguments":{"args":[]}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":7,"method":"does/not/exist"}"#,
        "\n",
    );
    let mut out = Vec::new();
    lancet::mcp::serve(Cursor::new(input.as_bytes()), &mut out, cfg).unwrap();
    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // The notification gets no reply.
    assert_eq!(lines.len(), 8);
    assert_eq!(lines[0]["error"]["code"], json!(-32700));
    assert_eq!(lines[1]["result"]["protocolVersion"], json!("2024-11-05"));
    assert_eq!(lines[2]["result"]["protocolVersion"], json!("2025-03-26"));
    assert_eq!(lines[3]["result"], json!({}));
    let tools = lines[4]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], json!("lancet"));
    assert_eq!(tools[0]["inputSchema"]["required"], json!(["args"]));
    assert_eq!(lines[5]["error"]["code"], json!(-32602)); // old per-command tool names
    assert_eq!(lines[6]["result"]["isError"], json!(true)); // empty args
    assert_eq!(lines[7]["error"]["code"], json!(-32601));
}

#[test]
fn mcp_runs_cli_commands_with_cli_output_and_exit_codes() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/a.rs"),
        "fn a() -> u32 {\n    let n = old(1);\n    n\n}\n",
    )
    .unwrap();
    fs::write(root.join("src/b.rs"), "fn b() { old(2); old(3); }\n").unwrap();
    let mut m = McpProc::spawn(root, &[]);
    m.request("initialize", json!({ "protocolVersion": "2025-03-26" }));
    m.notify("notifications/initialized", json!({}));

    // Same output as the CLI, paths relative to --root.
    let (text, err) = m.run(&["rg", "-w", "old", "src"], None);
    assert!(!err);
    assert_eq!(
        text,
        "src/a.rs:2:    let n = old(1);\nsrc/b.rs:1:fn b() { old(2); old(3); }\n"
    );
    let (text, err) = m.run(&["rg", "zzz", "src"], None);
    assert!(!err && text.contains("exit 1"), "{text}"); // nothing found is not an error
    let (text, err) = m.run(&["fd", ".", "nope"], None);
    assert!(err && text.contains("nope: No such file"), "{text}");
    let (text, _) = m.run(&["\"old\" in line", "src"], None);
    assert!(text.contains("src/b.rs"), "{text}");
    let (text, _) = m.run(&["read", "src/a.rs:2"], None);
    assert!(text.starts_with("# src/a.rs rev "), "{text}");

    // Edits: preview, apply with the rev, a stale rev is a conflict.
    let rev_a = m.rev("src/a.rs");
    let (text, err) = m.run(
        &[
            "edit", "src/a.rs", "--find", "old(1)", "--with", "new(1)", "--rev", &rev_a,
        ],
        None,
    );
    assert!(!err && text.contains("+    let n = new(1);"), "{text}");
    assert!(fs::read_to_string(root.join("src/a.rs"))
        .unwrap()
        .contains("old(1)"));
    let (_, err) = m.run(
        &[
            "edit", "src/a.rs", "--find", "old(1)", "--with", "new(1)", "--rev", &rev_a, "--apply",
        ],
        None,
    );
    assert!(!err);
    let (text, err) = m.run(
        &[
            "edit", "src/a.rs", "--find", "new(1)", "--with", "x(1)", "--rev", &rev_a, "--apply",
        ],
        None,
    );
    assert!(err && text.contains("changed since rev"), "{text}");

    // Several files through stdin.
    let hunks = json!([
        {"path": "src/a.rs", "rev": m.rev("src/a.rs"), "find": "new(1)", "with": "z(1)"},
        {"path": "src/b.rs", "rev": m.rev("src/b.rs"), "find": "old(", "with": "z(", "all": true},
    ])
    .to_string();
    let (text, err) = m.run(&["edit", "--edits", "-", "--apply"], Some(&hunks));
    assert!(!err, "{text}");
    assert_eq!(
        fs::read_to_string(root.join("src/b.rs")).unwrap(),
        "fn b() { z(2); z(3); }\n"
    );

    // A diff through stdin, a local rename, a structural rewrite.
    let diff =
        "--- a/src/b.rs\n+++ b/src/b.rs\n@@ -1 +1 @@\n-fn b() { z(2); z(3); }\n+fn b() { z(2); }\n";
    let (text, err) = m.run(&["patch", "-", "--apply"], Some(diff));
    assert!(!err, "{text}");
    let (text, err) = m.run(&["rename", "src/a.rs:2", "n", "total", "--apply"], None);
    assert!(!err, "{text}");
    assert!(fs::read_to_string(root.join("src/a.rs"))
        .unwrap()
        .contains("    total\n"));
    let (text, err) = m.run(
        &[
            "ast",
            "z($X)",
            "src",
            "--lang",
            "rs",
            "--rewrite",
            "w($X)",
            "--apply",
        ],
        None,
    );
    assert!(!err, "{text}");
    assert_eq!(
        fs::read_to_string(root.join("src/b.rs")).unwrap(),
        "fn b() { w(2); }\n"
    );

    // Output is capped.
    let (text, _) = m.run(&["read", "src/a.rs"], None);
    let full = text.len();
    let reply = m.request(
        "tools/call",
        json!({"name": "lancet", "arguments": {"args": ["read", "src/a.rs"], "max_output_bytes": 256}}),
    );
    assert!(
        full < 256
            || reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("truncated")
    );
}

#[test]
fn mcp_read_only_mode_blocks_all_mutations() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("a.rs"), "fn a() { old(1); }\n").unwrap();
    let mut m = McpProc::spawn(root, &["--read-only"]);
    m.request("initialize", json!({ "protocolVersion": "2025-03-26" }));
    let list = m.request("tools/list", json!({}));
    assert_eq!(
        list["result"]["tools"][0]["annotations"]["readOnlyHint"],
        json!(true)
    );
    for args in [
        &["edit", "a.rs", "--find", "old", "--with", "new", "--apply"][..],
        &["edit", "a.rs", "--find", "old", "--with", "new"],
        &["patch", "-"],
        &["rename", "a.rs:1", "a", "b"],
        &["index", "."],
        &[
            "ast",
            "old($X)",
            ".",
            "--lang",
            "rs",
            "--rewrite",
            "new($X)",
            "--apply",
        ],
    ] {
        let (text, err) = m.run(args, Some(""));
        assert!(err && text.contains("read-only"), "{args:?}: {text}");
    }
    assert_eq!(
        fs::read_to_string(root.join("a.rs")).unwrap(),
        "fn a() { old(1); }\n"
    );
    let (text, err) = m.run(&["read", "a.rs"], None);
    assert!(!err && text.contains("old(1)"));
    let (text, err) = m.run(
        &[
            "ast",
            "old($X)",
            ".",
            "--lang",
            "rs",
            "--rewrite",
            "new($X)",
        ],
        None,
    );
    assert!(!err && text.contains("+fn a() { new(1); }"), "{text}");
}
