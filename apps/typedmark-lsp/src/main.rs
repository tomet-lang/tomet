use std::collections::HashMap;

use lsp_server::{Connection, Message, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{Formatting, Request as _};
use lsp_types::{
    InitializeParams, OneOf, PublishDiagnosticsParams, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

fn main() -> anyhow::Result<()> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        document_formatting_provider: Some(OneOf::Left(true)),
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
    // diagnostics are re-derived from scratch each time -- no cache
    // needed for those. Formatting, though, is request-driven and only
    // gets a URI in its params, not the text, so open buffers have to be
    // cached somewhere to answer it; this map is that cache.
    let mut documents: HashMap<Uri, String> = HashMap::new();
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }
                if req.method == Formatting::METHOD {
                    let params: lsp_types::DocumentFormattingParams =
                        serde_json::from_value(req.params)?;
                    let edits = documents
                        .get(&params.text_document.uri)
                        .map(|text| typedmark_lsp::format_edits(text))
                        .unwrap_or_default();
                    let result = serde_json::to_value(edits)?;
                    connection
                        .sender
                        .send(Message::Response(Response::new_ok(req.id, result)))?;
                }
                // Everything else falls through unanswered by design (the
                // client won't send a request we didn't advertise support
                // for in `ServerCapabilities`).
            }
            Message::Notification(note) => match note.method.as_str() {
                DidOpenTextDocument::METHOD => {
                    let params: lsp_types::DidOpenTextDocumentParams =
                        serde_json::from_value(note.params)?;
                    let uri = params.text_document.uri;
                    documents.insert(uri.clone(), params.text_document.text.clone());
                    publish(&connection, uri, &params.text_document.text)?;
                }
                DidChangeTextDocument::METHOD => {
                    let params: lsp_types::DidChangeTextDocumentParams =
                        serde_json::from_value(note.params)?;
                    let uri = params.text_document.uri;
                    if let Some(change) = params.content_changes.into_iter().next_back() {
                        documents.insert(uri.clone(), change.text.clone());
                        publish(&connection, uri, &change.text)?;
                    }
                }
                DidCloseTextDocument::METHOD => {
                    let params: lsp_types::DidCloseTextDocumentParams =
                        serde_json::from_value(note.params)?;
                    let uri = params.text_document.uri;
                    documents.remove(&uri);
                    publish_diagnostics(&connection, uri, Vec::new())?;
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
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    let notification =
        lsp_server::Notification::new(PublishDiagnostics::METHOD.to_string(), params);
    connection
        .sender
        .send(Message::Notification(notification))?;
    Ok(())
}
