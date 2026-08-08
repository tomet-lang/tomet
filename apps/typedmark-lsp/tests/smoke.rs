//! Protocol-level smoke test: drives the actual `typedmark-lsp` binary
//! over stdio with real LSP framing, since the harness this was built in
//! can't launch an editor to test against (see `AGENTS.md`). Exercises
//! the full initialize -> didOpen -> didChange -> didClose -> shutdown
//! sequence and checks the diagnostics that come back at each step.

use serde_json::{json, Value};
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
        let mut child = Command::new(env!("CARGO_BIN_EXE_typedmark-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn typedmark-lsp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Server { child, stdin: Some(stdin), stdout, next_id: 1 }
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
        self.write_message(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
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
    assert_eq!(msg["method"], "textDocument/publishDiagnostics", "unexpected message: {msg:?}");
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
                "uri": "file:///tmp/bad.tm",
                "languageId": "typedmark",
                "version": 1,
                "text": "<caution>[ unterminated\n",
            }
        }),
    );
    let diags = server.read_message();
    let diags = diagnostics_array(&diags);
    assert_eq!(diags.len(), 1, "expected one diagnostic for a broken document: {diags:?}");
    assert_eq!(diags[0]["source"], "typedmark");

    server.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": "file:///tmp/bad.tm", "version": 2},
            "contentChanges": [{"text": "#[ Hello ]\n"}],
        }),
    );
    let diags = server.read_message();
    assert!(diagnostics_array(&diags).is_empty(), "expected no diagnostics once fixed");

    server.send_notification(
        "textDocument/didClose",
        json!({"textDocument": {"uri": "file:///tmp/bad.tm"}}),
    );
    let diags = server.read_message();
    assert!(diagnostics_array(&diags).is_empty(), "expected diagnostics cleared on close");

    server.shutdown();
}
