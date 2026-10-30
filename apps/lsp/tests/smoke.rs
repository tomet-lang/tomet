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
                "text": "@caution[ unterminated\n",
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
                "text": "=[ Header ]#(h1)\n\n@deck.callout(type: info)[ Message ]\n\n@deck.ref(id: h1)\n",
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
                "text": "::macros{\n  gh: \"https://github.com/tomet/tomet/issues/${1}\"\n}\n\n$macro.gh(101)\n",
            }
        }),
    );
    let _diags = server.read_message();

    let hover_resp = server.send_request(
        "textDocument/hover",
        json!({
            "textDocument": {"uri": "file:///tmp/macro_test.tmt"},
            "position": {"line": 4, "character": 2},
        }),
    );
    let hover_val = hover_resp["result"]["contents"]["value"]
        .as_str()
        .expect("hover value string");
    assert!(hover_val.contains("Macro Result"));
    assert!(hover_val.contains("https://github.com/tomet/tomet/issues/101"));

    server.shutdown();
}

#[test]
fn macro_hover_does_not_take_macros_from_an_imported_config_over_stdio() {
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
        "::macros{\n  youtube_video: \"https://www.youtube.com/watch?v=${1}\"\n}\n",
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
                "text": "::config(import:\"custom.config.tmt\")\n\n@embed($macro.youtube_video(\"Pm_h6FnF8HU\"))[Video]\n",
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
    // Hover shows what rendering shows, and rendering takes its macros from
    // the document's own `::config` and the vault's settings -- not from a
    // file the document imports. So the imported macro is not there, and the
    // hover says so rather than showing a value the output would not have.
    assert!(
        hover_val.contains("Error"),
        "an imported macro is not one rendering knows: {hover_val}"
    );
    assert!(!hover_val.contains("https://www.youtube.com/watch?v=Pm_h6FnF8HU"));

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
    let config_path = dir.join(tomet_load_config::VAULT_CONFIG_FILE);
    std::fs::write(
        &config_path,
        "::macros{\n  twitter_post: \"https://x.com/${1}/status/${2}\"\n}\n",
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
                "text": "@embed($macro.twitter_post(\"kosekibijou\", \"1807568682631254496\"))[Bijou]\n",
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
    assert!(
        hover_val.contains("Macro Result"),
        "got hover_val: {hover_val}"
    );
    assert!(hover_val.contains("https://x.com/kosekibijou/status/1807568682631254496"));

    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A vault in a temp directory whose settings declare `id` a `type: uuid`.
fn uuid_vault(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("tomet_lsp_smoke_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join(tomet_load_config::VAULT_CONFIG_FILE),
        "::kind(config)\n::config{ meta: { id: { type: uuid } } }\n",
    )
    .unwrap();
    root
}

fn open(server: &mut Server, path: &std::path::Path, text: &str) {
    server.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": format!("file://{}", path.display()),
                "languageId": "tomet",
                "version": 1,
                "text": text,
            }
        }),
    );
}

fn initialize(server: &mut Server, capabilities: Value) {
    let init = server.send_request(
        "initialize",
        json!({"processId": Value::Null, "rootUri": Value::Null, "capabilities": capabilities}),
    );
    assert!(init.get("error").is_none(), "initialize failed: {init:?}");
    server.send_notification("initialized", json!({}));
}

/// What the vault says -- here, the settings' `type: uuid` -- arrives after
/// the typing pauses, on top of what the text alone said at once.
#[test]
fn the_vaults_diagnostics_follow_the_text_after_a_pause() {
    let root = uuid_vault("pause");
    let path = root.join("a.tmt");
    let mut server = Server::start();
    initialize(&mut server, json!({}));

    open(&mut server, &path, "::id(not-a-uuid)\n");
    let at_once = server.read_message();
    assert!(
        diagnostics_array(&at_once).is_empty(),
        "the text alone has nothing to say: {at_once:?}"
    );

    let after_pause = server.read_message();
    let diags = diagnostics_array(&after_pause);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["severity"], 1);
    assert!(
        diags[0]["message"]
            .as_str()
            .unwrap()
            .contains("not a valid uuid"),
        "{diags:?}"
    );

    server.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

/// A client that can register watchers is asked to watch `.tmt` files.
#[test]
fn a_client_that_can_watch_files_is_asked_to() {
    let mut server = Server::start();
    initialize(
        &mut server,
        json!({"workspace": {"didChangeWatchedFiles": {"dynamicRegistration": true}}}),
    );
    let request = server.read_message();
    assert_eq!(
        request["method"], "client/registerCapability",
        "{request:?}"
    );
    let registration = &request["params"]["registrations"][0];
    assert_eq!(registration["method"], "workspace/didChangeWatchedFiles");
    assert_eq!(
        registration["registerOptions"]["watchers"][0]["globPattern"],
        "**/*.tmt"
    );
    server.shutdown();
}

/// The settings change on the disk: the diagnostics are asked for again.
#[test]
fn a_changed_config_changes_the_diagnostics() {
    let root = uuid_vault("config");
    let path = root.join("a.tmt");
    let mut server = Server::start();
    initialize(&mut server, json!({}));

    open(&mut server, &path, "::id(not-a-uuid)\n");
    let _at_once = server.read_message();
    let first = server.read_message();
    assert_eq!(diagnostics_array(&first).len(), 1, "{first:?}");

    // The declaration is dropped, and the server is told a file changed.
    std::fs::write(
        root.join(tomet_load_config::VAULT_CONFIG_FILE),
        "::kind(config)\n",
    )
    .unwrap();
    server.send_notification(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{
            "uri": format!("file://{}", root.join(tomet_load_config::VAULT_CONFIG_FILE).display()),
            "type": 2,
        }]}),
    );
    let second = server.read_message();
    assert!(
        diagnostics_array(&second).is_empty(),
        "nothing declares `id` any more: {second:?}"
    );

    server.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

/// A vocabulary the config declares and that is not there is the vault's
/// problem, not a document's: it goes to the client's log, once.
#[test]
fn a_vault_with_a_missing_vocabulary_says_so_in_the_log() {
    let root = std::env::temp_dir().join(format!("tomet_lsp_smoke_notice_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join(tomet_load_config::VAULT_CONFIG_FILE),
        "::kind(config)\n::config{ vocabularies: list(\".tomet/vocabularies/missing.vocabulary.tmt\") }\n",
    )
    .unwrap();
    let path = root.join("a.tmt");
    let mut server = Server::start();
    initialize(&mut server, json!({}));

    open(&mut server, &path, "Text.\n");
    let at_once = server.read_message();
    assert!(diagnostics_array(&at_once).is_empty(), "{at_once:?}");

    let logged = server.read_message();
    assert_eq!(logged["method"], "window/logMessage", "{logged:?}");
    assert_eq!(logged["params"]["type"], 2, "a warning: {logged:?}");
    let message = logged["params"]["message"].as_str().unwrap();
    assert!(message.contains("missing.vocabulary.tmt"), "{message}");

    server.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

/// A `.ctmt` is data: the cotomet parser's diagnostics and the whitespace
/// formatter, and nothing a document gets (no hover, symbols, completion),
/// and the vault never hears of it -- here the vault would call the file
/// invalid, since `id` has to be a UUID.
#[test]
fn a_cotomet_file_is_served_as_data_and_never_reaches_the_vault() {
    let root = uuid_vault("ctmt");
    let path = root.join("catasto.ctmt");
    let uri = format!("file://{}", path.display());
    let mut server = Server::start();
    initialize(&mut server, json!({}));

    open(&mut server, &path, "id: not-a-uuid\nstray\n");
    let at_once = server.read_message();
    let diags = diagnostics_array(&at_once);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(
        diags[0]["message"].as_str().unwrap().contains("key: value"),
        "{diags:?}"
    );
    assert_eq!(
        diags[0]["range"]["start"],
        json!({"line": 1, "character": 0})
    );

    // Nothing else is published for it, not even after the vault's pause. The
    // answer to the next request is the next message the server sends.
    std::thread::sleep(std::time::Duration::from_millis(600));
    let position = json!({"textDocument": {"uri": uri}, "position": {"line": 0, "character": 1}});
    for method in [
        "textDocument/hover",
        "textDocument/definition",
        "textDocument/completion",
    ] {
        let answer = server.send_request(method, position.clone());
        assert_eq!(answer["id"].is_number(), true, "{method}: {answer:?}");
        assert!(answer["result"].is_null(), "{method}: {answer:?}");
    }
    for method in ["textDocument/documentSymbol", "textDocument/foldingRange"] {
        let answer = server.send_request(method, json!({"textDocument": {"uri": uri}}));
        assert!(answer["result"].is_null(), "{method}: {answer:?}");
    }

    // Fixed, and ragged: the whitespace formatter answers
    server.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": "// c  \r\nid: x   \r\n\r\n\r\nm: \"a  \r\nb\"\r\n"}],
        }),
    );
    let fixed = server.read_message();
    assert!(diagnostics_array(&fixed).is_empty(), "{fixed:?}");
    let formatted = server.send_request(
        "textDocument/formatting",
        json!({"textDocument": {"uri": uri}, "options": {"tabSize": 2, "insertSpaces": true}}),
    );
    assert_eq!(
        formatted["result"][0]["newText"], "// c\nid: x\n\nm: \"a  \r\nb\"\n",
        "{formatted:?}"
    );

    server.send_notification(
        "textDocument/didClose",
        json!({"textDocument": {"uri": uri}}),
    );
    let closed = server.read_message();
    assert!(diagnostics_array(&closed).is_empty(), "{closed:?}");

    server.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}
