use lsp_server::{Connection, Message};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::{
    InitializeParams, PublishDiagnosticsParams, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};

fn main() -> anyhow::Result<()> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..Default::default()
    };
    let init_params = connection.initialize(serde_json::to_value(capabilities)?)?;
    let _init_params: InitializeParams = serde_json::from_value(init_params)?;

    main_loop(connection)?;
    io_threads.join()?;
    Ok(())
}

/// Takes `connection` by value so it (and its `sender`) is dropped when
/// this returns -- `io_threads.join()` waits on the writer thread's
/// channel hanging up, which only happens once every `Sender` is gone,
/// so a version of this that kept `connection` alive in `main` (e.g. by
/// taking `&Connection` here) would deadlock on shutdown.
fn main_loop(connection: Connection) -> anyhow::Result<()> {
    // Full-sync mode sends the whole new text on every open/change, so
    // there's no per-document state worth keeping -- diagnostics are
    // re-derived from scratch each time, no cache needed.
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }
                // No requests are handled in the diagnostics-only v1;
                // everything else falls through unanswered by design
                // (the client won't send anything we didn't advertise
                // support for in `ServerCapabilities`).
            }
            Message::Notification(note) => match note.method.as_str() {
                DidOpenTextDocument::METHOD => {
                    let params: lsp_types::DidOpenTextDocumentParams =
                        serde_json::from_value(note.params)?;
                    publish(&connection, params.text_document.uri, &params.text_document.text)?;
                }
                DidChangeTextDocument::METHOD => {
                    let params: lsp_types::DidChangeTextDocumentParams =
                        serde_json::from_value(note.params)?;
                    let uri = params.text_document.uri;
                    if let Some(change) = params.content_changes.into_iter().next_back() {
                        publish(&connection, uri, &change.text)?;
                    }
                }
                DidCloseTextDocument::METHOD => {
                    let params: lsp_types::DidCloseTextDocumentParams =
                        serde_json::from_value(note.params)?;
                    publish_diagnostics(&connection, params.text_document.uri, Vec::new())?;
                }
                _ => {}
            },
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn publish(connection: &Connection, uri: Uri, text: &str) -> anyhow::Result<()> {
    let diagnostics = typedmark_lsp::diagnostics_for(text);
    publish_diagnostics(connection, uri, diagnostics)
}

fn publish_diagnostics(
    connection: &Connection,
    uri: Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> anyhow::Result<()> {
    let params = PublishDiagnosticsParams { uri, diagnostics, version: None };
    let notification =
        lsp_server::Notification::new(PublishDiagnostics::METHOD.to_string(), params);
    connection.sender.send(Message::Notification(notification))?;
    Ok(())
}
