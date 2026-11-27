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

    fn call_tool(&mut self, name: &str, args: Value) -> (Value, bool) {
        let reply = self.request(
            "tools/call",
            json!({
                "name": name,
                "arguments": args,
            }),
        );
        assert!(
            reply.get("error").is_none(),
            "unexpected RPC error: {reply}"
        );
        let res = &reply["result"];
        let is_err = res["isError"].as_bool().unwrap();
        let text = res["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        (payload, is_err)
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
    let cfg = lancet::mcp::McpConfig::new(dir.path());
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
        r#"{"jsonrpc":"2.0","id":4,"method":"prompts/list"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":5,"method":"prompts/get","params":{"name":"lancet-safe-edit","arguments":{"target":"src/lib.rs"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":6,"method":"does/not/exist"}"#,
        "\n",
    );
    let mut out = Vec::new();
    lancet::mcp::serve(Cursor::new(input.as_bytes()), &mut out, cfg).unwrap();

    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // Notification emitted no line, so 7 replies total.
    assert_eq!(lines.len(), 7);
    assert_eq!(lines[0]["error"]["code"], json!(-32700));
    assert_eq!(lines[1]["result"]["protocolVersion"], json!("2024-11-05"));
    assert_eq!(lines[2]["result"]["protocolVersion"], json!("2025-03-26"));
    assert_eq!(lines[3]["result"], json!({}));
    assert_eq!(lines[4]["result"]["prompts"].as_array().unwrap().len(), 2);
    assert!(lines[5]["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap()
        .contains("src/lib.rs"));
    assert_eq!(lines[6]["error"]["code"], json!(-32601));
}

#[test]
fn mcp_end_to_end_search_read_cas_edit_ast_and_patch() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "/// Greets a caller.\npub fn greet(name: &str) -> String {\n    let prefix = Some(\"hello\").unwrap();\n    format!(\"{prefix} {name}\")\n}\n\npub fn helper() -> usize {\n    greet(\"world\").len()\n}\n",
    )
    .unwrap();
    fs::write(root.join("docs/notes.txt"), "line 1\nline 2\nline 3\n").unwrap();

    let mut mcp = McpProc::spawn(root, &[]);
    let init = mcp.request(
        "initialize",
        json!({ "protocolVersion": "2025-03-26", "clientInfo": { "name": "test" } }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], json!("lancet"));
    mcp.notify("notifications/initialized", json!({}));

    let tools_reply = mcp.request("tools/list", json!({}));
    let names: Vec<&str> = tools_reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 11);
    assert!(names.contains(&"lancet_edit"));
    assert!(names.contains(&"lancet_patch"));
    assert!(names.contains(&"lancet_rename"));

    // Index lifecycle
    let (st0, err) = mcp.call_tool("lancet_index", json!({ "action": "status" }));
    assert!(!err);
    assert_eq!(st0["exists"], json!(false));

    let (synced, err) = mcp.call_tool("lancet_index", json!({ "action": "sync" }));
    assert!(!err);
    assert_eq!(synced["status"], json!("complete"));

    let (st1, err) = mcp.call_tool("lancet_index", json!({ "action": "status" }));
    assert!(!err);
    assert_eq!(st1["exists"], json!(true));

    // Discovery, outline, search, rg, refs
    let (fd_res, err) = mcp.call_tool("lancet_fd", json!({ "exts": ["rs"], "tree": true }));
    assert!(!err);
    assert_eq!(fd_res["status"], json!("complete"));

    let (out_res, err) = mcp.call_tool("lancet_outline", json!({ "path": "src/lib.rs" }));
    assert!(!err);
    assert_eq!(out_res["status"], json!("complete"));

    let (search_res, err) =
        mcp.call_tool("lancet_search", json!({ "query": "\"greet\" in function" }));
    assert!(!err);
    assert_eq!(search_res["status"], json!("complete"));

    let (rg_hit, err) = mcp.call_tool("lancet_rg", json!({ "pattern": "greet" }));
    assert!(!err);
    assert_eq!(rg_hit["matched"], json!(true));
    assert!(rg_hit["output"].as_str().unwrap().contains("src/lib.rs"));

    let (rg_miss, err) = mcp.call_tool("lancet_rg", json!({ "pattern": "no_such_token_xyz" }));
    assert!(!err);
    assert_eq!(rg_miss["status"], json!("empty"));
    assert_eq!(rg_miss["matched"], json!(false));

    // Read by symbol and inline range, capturing rev
    let (read_inline, err) = mcp.call_tool("lancet_read", json!({ "target": "src/lib.rs:1-2" }));
    assert!(!err);
    assert_eq!(read_inline["start_line"], json!(1));
    assert_eq!(read_inline["end_line"], json!(2));

    let (read_sym, err) = mcp.call_tool(
        "lancet_read",
        json!({ "target": "src/lib.rs", "symbol": "helper" }),
    );
    assert!(!err);
    let rev0 = read_sym["rev"].as_str().unwrap().to_string();

    // Dry-run edit leaves disk untouched
    let (dry, err) = mcp.call_tool(
        "lancet_edit",
        json!({
            "path": "src/lib.rs",
            "symbol": "helper",
            "with": "pub fn helper() -> usize {\n    greet(\"lancet\").len()\n}",
            "rev": rev0,
            "apply": false
        }),
    );
    assert!(!err);
    assert_eq!(dry["applied"], json!(false));
    assert!(fs::read_to_string(root.join("src/lib.rs"))
        .unwrap()
        .contains("world"));

    // Apply edit with matching rev succeeds and returns rev_after
    let (applied, err) = mcp.call_tool(
        "lancet_edit",
        json!({
            "path": "src/lib.rs",
            "symbol": "helper",
            "with": "pub fn helper() -> usize {\n    greet(\"lancet\").len()\n}",
            "rev": rev0,
            "apply": true
        }),
    );
    assert!(!err);
    assert_eq!(applied["applied"], json!(true));
    let rev1 = applied["rev_after"].as_str().unwrap().to_string();
    assert_ne!(rev0, rev1);

    // Reusing stale rev0 triggers conflict (isError: true)
    let (conflict, err) = mcp.call_tool(
        "lancet_edit",
        json!({
            "path": "src/lib.rs",
            "symbol": "helper",
            "with": "pub fn helper() -> usize { 0 }",
            "rev": rev0,
            "apply": true
        }),
    );
    assert!(err);
    assert_eq!(conflict["status"], json!("conflict"));

    // Find target + multi-hunk edits
    let (find_res, err) = mcp.call_tool(
        "lancet_edit",
        json!({
            "path": "src/lib.rs",
            "find": "greet(\"lancet\").len()",
            "with": "greet(\"mcp\").len()",
            "rev": rev1,
            "apply": true
        }),
    );
    assert!(!err);
    let rev2 = find_res["rev_after"].as_str().unwrap().to_string();

    let (multi_res, err) = mcp.call_tool(
        "lancet_edit",
        json!({
            "path": "src/lib.rs",
            "edits": [
                { "find": "\"hello\"", "with": "\"hi\"" },
                { "find": "\"mcp\"", "with": "\"world\"" }
            ],
            "rev": rev2,
            "apply": true
        }),
    );
    assert!(!err);
    assert_eq!(multi_res["applied"], json!(true));

    // AST search + rewrite (dry-run then apply)
    let (ast_dry, err) = mcp.call_tool(
        "lancet_ast",
        json!({
            "pattern": "$X.unwrap()",
            "lang": "rs",
            "rewrite": "$X.expect(\"ok\")",
            "apply": false
        }),
    );
    assert!(!err);
    assert_eq!(ast_dry["matched"], json!(1));
    assert!(!fs::read_to_string(root.join("src/lib.rs"))
        .unwrap()
        .contains("expect"));

    let (ast_apply, err) = mcp.call_tool(
        "lancet_ast",
        json!({
            "pattern": "$X.unwrap()",
            "lang": "rs",
            "rewrite": "$X.expect(\"ok\")",
            "apply": true
        }),
    );
    assert!(!err);
    assert_eq!(ast_apply["matched"], json!(1));
    assert!(fs::read_to_string(root.join("src/lib.rs"))
        .unwrap()
        .contains("expect(\"ok\")"));

    // Patch apply
    let diff = "--- a/docs/notes.txt\n+++ b/docs/notes.txt\n@@ -1,3 +1,3 @@\n line 1\n-line 2\n+line TWO\n line 3\n";
    let (patch_res, err) = mcp.call_tool("lancet_patch", json!({ "patch": diff, "apply": true }));
    assert!(!err);
    assert_eq!(patch_res["applied"], json!(true));
    assert!(fs::read_to_string(root.join("docs/notes.txt"))
        .unwrap()
        .contains("line TWO"));

    // Local scope rename (dry-run then apply)
    fs::write(
        root.join("src/local.rs"),
        "pub fn calc(x: i32) -> i32 {\n    let tmp = x + 1;\n    tmp * 2\n}\n",
    )
    .unwrap();
    let (ren_dry, err) = mcp.call_tool(
        "lancet_rename",
        json!({
            "path": "src/local.rs",
            "line": 2,
            "old": "tmp",
            "new": "scaled",
            "apply": false
        }),
    );
    assert!(!err);
    assert_eq!(ren_dry["applied"], json!(false));
    assert_eq!(ren_dry["sites"].as_array().unwrap().len(), 2);
    assert!(fs::read_to_string(root.join("src/local.rs"))
        .unwrap()
        .contains("let tmp ="));

    let (ren_apply, err) = mcp.call_tool(
        "lancet_rename",
        json!({
            "path": "src/local.rs",
            "line": 2,
            "old": "tmp",
            "new": "scaled",
            "apply": true
        }),
    );
    assert!(!err);
    assert_eq!(ren_apply["applied"], json!(true));
    let after_ren = fs::read_to_string(root.join("src/local.rs")).unwrap();
    assert!(after_ren.contains("let scaled = x + 1;\n    scaled * 2"));

    // Refs & rg truncation check
    let (refs_res, err) = mcp.call_tool("lancet_refs", json!({ "symbol": "greet" }));
    assert!(!err);
    assert_eq!(refs_res["definitions"].as_array().unwrap().len(), 1);

    let big = "match_token_line\n".repeat(40);
    fs::write(root.join("docs/big.txt"), big).unwrap();
    let (trunc_rg, err) = mcp.call_tool(
        "lancet_rg",
        json!({ "pattern": "match_token_line", "max_output_bytes": 256 }),
    );
    assert!(!err);
    assert_eq!(trunc_rg["truncated"], json!(true));
    assert!(trunc_rg["output"].as_str().unwrap().len() <= 256);
}

#[test]
fn mcp_read_only_mode_blocks_all_mutations() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    let orig = "pub fn f() -> usize { Some(1).unwrap() }\n";
    fs::write(root.join("src/lib.rs"), orig).unwrap();

    let mut mcp = McpProc::spawn(root, &["--read-only"]);
    let _ = mcp.request("initialize", json!({ "protocolVersion": "2025-03-26" }));
    mcp.notify("notifications/initialized", json!({}));

    let tools_reply = mcp.request("tools/list", json!({}));
    let names: Vec<&str> = tools_reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"lancet_edit"));
    assert!(!names.contains(&"lancet_patch"));
    assert!(!names.contains(&"lancet_rename"));
    assert_eq!(names.len(), 8);

    // Read & index status still work
    let (r_res, err) = mcp.call_tool("lancet_read", json!({ "target": "src/lib.rs" }));
    assert!(!err);
    let rev = r_res["rev"].as_str().unwrap();

    let (_, err) = mcp.call_tool("lancet_index", json!({ "action": "status" }));
    assert!(!err);

    // All write calls are refused with isError: true
    let (_, err) = mcp.call_tool(
        "lancet_edit",
        json!({ "path": "src/lib.rs", "lines": "1", "with": "// nope", "rev": rev, "apply": true }),
    );
    assert!(err);

    let (_, err) = mcp.call_tool(
        "lancet_patch",
        json!({ "patch": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn f() -> usize { Some(1).unwrap() }\n+// x\n", "apply": true }),
    );
    assert!(err);

    let (_, err) = mcp.call_tool(
        "lancet_ast",
        json!({ "pattern": "$X.unwrap()", "lang": "rs", "rewrite": "$X?", "apply": true }),
    );
    assert!(err);

    let (_, err) = mcp.call_tool("lancet_index", json!({ "action": "sync" }));
    assert!(err);

    let (_, err) = mcp.call_tool(
        "lancet_rename",
        json!({ "path": "src/lib.rs", "line": 1, "old": "f", "new": "g", "apply": true }),
    );
    assert!(err);

    assert_eq!(fs::read_to_string(root.join("src/lib.rs")).unwrap(), orig);
}
