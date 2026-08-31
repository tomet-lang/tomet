//! Protocol-level smoke test: drives the actual `tomet-lsp` binary
//! over stdio with real LSP framing, since the harness this was built in
//! can't launch an editor to test against (see `AGENTS.md`). Exercises
//! the full initialize -> didOpen -> didChange -> didClose -> shutdown
//! sequence and checks the diagnostics that come back at each step.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

struct Server {
    child: Child,
    // `Option` so `shutdown` can drop (close) the write half on its own,
    // same as a real client disconnecting -- the server's reader thread
    // blocks on stdin until EOF, so without this `Connection::stdio`'s
    // `io_threads.join()` never returns and the process never exits even
    // after it's handled the `exit` notification.
    stdin: Option<ChildStdin>,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: i64,
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tomet-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn tomet-lsp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Server {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 1,
        }
    }

    fn write_message(&mut self, value: &Value) {
        let body = serde_json::to_string(value).unwrap();
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        write!(stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        stdin.flush().unwrap();
    }

    fn send_request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        );
        self.read_message()
    }

    fn send_notification(&mut self, method: &str, params: Value) {
        self.write_message(&json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn read_message(&mut self) -> Value {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(v) = line.strip_prefix("Content-Length:") {
                content_length = Some(v.trim().parse::<usize>().unwrap());
            }
        }
        let len = content_length.expect("no Content-Length header");
        let mut buf = vec![0u8; len];
        self.stdout.read_exact(&mut buf).unwrap();
        serde_json::from_slice(&buf).unwrap()
    }

    fn shutdown(mut self) {
        let resp = self.send_request("shutdown", Value::Null);
        assert!(resp.get("error").is_none(), "shutdown failed: {resp:?}");
        self.send_notification("exit", Value::Null);
        self.stdin.take(); // close the write half, as a real client would
        let status = self.child.wait().unwrap();
        assert!(status.success(), "server exited with {status:?}");
    }
}

fn diagnostics_array(msg: &Value) -> &Vec<Value> {
    assert_eq!(
        msg["method"], "textDocument/publishDiagnostics",
        "unexpected message: {msg:?}"
    );
    msg["params"]["diagnostics"].as_array().unwrap()
}

#[test]
fn diagnostics_lifecycle_over_stdio() {
    let mut server = Server::start();

    let init = server.send_request(
        "initialize",
        json!({"processId": Value::Null, "rootUri": Value::Null, "capabilities": {}}),
    );
    assert!(init.get("error").is_none(), "initialize failed: {init:?}");
    server.send_notification("initialized", json!({}));

    server.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///tmp/bad.tmt",
                "languageId": "tomet",
                "version": 1,
                "text": "<caution>[ unterminated\n",
            }
        }),
    );
    let diags = server.read_message();
    let diags = diagnostics_array(&diags);
    assert_eq!(
        diags.len(),
        1,
        "expected one diagnostic for a broken document: {diags:?}"
    );
    assert_eq!(diags[0]["source"], "tomet");

    server.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": "file:///tmp/bad.tmt", "version": 2},
            "contentChanges": [{"text": "#[ Hello ]\n"}],
        }),
    );
    let diags = server.read_message();
    assert!(
        diagnostics_array(&diags).is_empty(),
        "expected no diagnostics once fixed"
    );

    server.send_notification(
        "textDocument/didClose",
        json!({"textDocument": {"uri": "file:///tmp/bad.tmt"}}),
    );
    let diags = server.read_message();
    assert!(
        diagnostics_array(&diags).is_empty(),
        "expected diagnostics cleared on close"
    );

    server.shutdown();
}

#[test]
fn formatting_request_returns_a_whole_document_edit() {
    let mut server = Server::start();

    let init = server.send_request(
        "initialize",
        json!({"processId": Value::Null, "rootUri": Value::Null, "capabilities": {}}),
    );
    assert!(init.get("error").is_none(), "initialize failed: {init:?}");
    assert_eq!(
        init["result"]["capabilities"]["documentFormattingProvider"], true,
        "server should advertise formatting support"
    );
    server.send_notification("initialized", json!({}));

    server.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///tmp/messy.tmt",
                "languageId": "tomet",
                "version": 1,
                "text": "#[ Hello ]  \n\n\n\n- one\n",
            }
        }),
    );
    let _diags = server.read_message(); // valid doc, published unconditionally

    let resp = server.send_request(
        "textDocument/formatting",
        json!({
            "textDocument": {"uri": "file:///tmp/messy.tmt"},
            "options": {"tabSize": 2, "insertSpaces": true},
        }),
    );
    assert!(
        resp.get("error").is_none(),
        "formatting request failed: {resp:?}"
    );
    let edits = resp["result"].as_array().expect("expected an edits array");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["newText"], "#[ Hello ]\n\n- one\n");

    server.shutdown();
}

#[test]
fn hover_symbols_definition_completion_over_stdio() {
    let mut server = Server::start();

    let init = server.send_request(
        "initialize",
        json!({"processId": Value::Null, "rootUri": Value::Null, "capabilities": {}}),
    );
    assert!(init.get("error").is_none(), "initialize failed: {init:?}");
    let caps = &init["result"]["capabilities"];
    assert_eq!(caps["hoverProvider"], true);
    assert_eq!(caps["documentSymbolProvider"], true);
    assert_eq!(caps["definitionProvider"], true);
    assert!(caps["completionProvider"].is_object());
    server.send_notification("initialized", json!({}));

    server.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///tmp/doc.tmt",
                "languageId": "tomet",
                "version": 1,
                "text": "#[ Header ]{id: h1}\n\n<callout>(type: info)[ Message ]\n\n@(id: h1)\n",
            }
        }),
    );
    let _diags = server.read_message();

    // Hover
    let hover_resp = server.send_request(
        "textDocument/hover",
        json!({
            "textDocument": {"uri": "file:///tmp/doc.tmt"},
            "position": {"line": 2, "character": 2},
        }),
    );
    assert!(
        hover_resp["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("callout")
    );

    // Document Symbols
    let symbols_resp = server.send_request(
        "textDocument/documentSymbol",
        json!({
            "textDocument": {"uri": "file:///tmp/doc.tmt"},
        }),
    );
    let symbols = symbols_resp["result"].as_array().unwrap();
    assert_eq!(symbols.len(), 3);

    // Goto Definition
    let def_resp = server.send_request(
        "textDocument/definition",
        json!({
            "textDocument": {"uri": "file:///tmp/doc.tmt"},
            "position": {"line": 4, "character": 4},
        }),
    );
    assert!(def_resp["result"].is_object());

    // Completion
    let comp_resp = server.send_request(
        "textDocument/completion",
        json!({
            "textDocument": {"uri": "file:///tmp/doc.tmt"},
            "position": {"line": 0, "character": 0},
        }),
    );
    assert!(!comp_resp["result"].as_array().unwrap().is_empty());

    server.shutdown();
}

#[test]
fn macro_hover_over_stdio() {
    let mut server = Server::start();

    let init = server.send_request(
        "initialize",
        json!({"processId": Value::Null, "rootUri": Value::Null, "capabilities": {}}),
    );
    assert!(init.get("error").is_none());
    server.send_notification("initialized", json!({}));

    server.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///tmp/macro_test.tmt",
                "languageId": "tomet",
                "version": 1,
                "text": "@config{\n  macros: {\n    gh: \"https://github.com/tomet-lang/tomet/issues/${1}\"\n  }\n}\n\n$gh(101)\n",
            }
        }),
    );
    let _diags = server.read_message();

    let hover_resp = server.send_request(
        "textDocument/hover",
        json!({
            "textDocument": {"uri": "file:///tmp/macro_test.tmt"},
            "position": {"line": 6, "character": 2},
        }),
    );
    let hover_val = hover_resp["result"]["contents"]["value"]
        .as_str()
        .expect("hover value string");
    assert!(hover_val.contains("Macro Result"));
    assert!(hover_val.contains("https://github.com/tomet-lang/tomet/issues/101"));

    server.shutdown();
}

#[test]
fn macro_hover_with_config_import_over_stdio() {
    let unique = format!(
        "tomet_smoke_import_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("custom.config.tmt");
    std::fs::write(
        &config_path,
        "@config(format:json){\n  {\n    \"macros\": {\n      \"youtube_video\": \"https://www.youtube.com/watch?v=${1}\"\n    }\n  }\n}\n",
    )
    .unwrap();

    let doc_path = dir.join("journal.tmt");
    let doc_uri = format!("file://{}", doc_path.display());

    let mut server = Server::start();

    let init = server.send_request(
        "initialize",
        json!({"processId": Value::Null, "rootUri": Value::Null, "capabilities": {}}),
    );
    assert!(init.get("error").is_none());
    server.send_notification("initialized", json!({}));

    server.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": doc_uri,
                "languageId": "tomet",
                "version": 1,
                "text": "@config(import:\"custom.config.tmt\")\n\n<embed>($youtube_video(\"Pm_h6FnF8HU\"))[Video]\n",
            }
        }),
    );
    let _diags = server.read_message();

    let hover_resp = server.send_request(
        "textDocument/hover",
        json!({
            "textDocument": {"uri": doc_uri},
            "position": {"line": 2, "character": 12},
        }),
    );
    let hover_val = hover_resp["result"]["contents"]["value"]
        .as_str()
        .unwrap_or_else(|| panic!("expected hover value string, got {hover_resp:?}"));
    assert!(hover_val.contains("Macro Result"), "got hover_val: {hover_val}");
    assert!(hover_val.contains("https://www.youtube.com/watch?v=Pm_h6FnF8HU"));

    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn macro_hover_with_workspace_auto_config_over_stdio() {
    let unique = format!(
        "tomet_smoke_auto_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("default.config.tmt");
    std::fs::write(
        &config_path,
        "@config(format:json){\n  {\n    \"macros\": {\n      \"twitter_post\": \"https://x.com/${1}/status/${2}\"\n    }\n  }\n}\n",
    )
    .unwrap();

    let doc_path = dir.join("sub/daily.tmt");
    std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
    let doc_uri = format!("file://{}", doc_path.display());

    let mut server = Server::start();

    let init = server.send_request(
        "initialize",
        json!({"processId": Value::Null, "rootUri": Value::Null, "capabilities": {}}),
    );
    assert!(init.get("error").is_none());
    server.send_notification("initialized", json!({}));

    server.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": doc_uri,
                "languageId": "tomet",
                "version": 1,
                // Zero configuration headers in note
                "text": "<embed>($twitter_post(\"kosekibijou\", \"1807568682631254496\"))[Bijou]\n",
            }
        }),
    );
    let _diags = server.read_message();

    let hover_resp = server.send_request(
        "textDocument/hover",
        json!({
            "textDocument": {"uri": doc_uri},
            "position": {"line": 0, "character": 12},
        }),
    );
    let hover_val = hover_resp["result"]["contents"]["value"]
        .as_str()
        .unwrap_or_else(|| panic!("expected hover value string, got {hover_resp:?}"));
    assert!(hover_val.contains("Macro Result"), "got hover_val: {hover_val}");
    assert!(hover_val.contains("https://x.com/kosekibijou/status/1807568682631254496"));

    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
