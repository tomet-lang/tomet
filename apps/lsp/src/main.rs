use std::collections::HashMap;

use lsp_server::{Connection, Message, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest, Request as _,
};
use lsp_types::{
    CompletionOptions, HoverProviderCapability, InitializeParams, OneOf, PublishDiagnosticsParams,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

fn main() -> anyhow::Result<()> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        document_formatting_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(vec!["<".to_string(), "@".to_string(), "$".to_string()]),
            work_done_progress_options: Default::default(),
            all_commit_characters: None,
            completion_item: None,
        }),
        ..Default::default()
    };
    let init_params = connection.initialize(serde_json::to_value(capabilities)?)?;
    let _init_params: InitializeParams = serde_json::from_value(init_params)?;

    main_loop(connection)?;
    io_threads.join()?;
    Ok(())
}

fn main_loop(connection: Connection) -> anyhow::Result<()> {
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
                        .map(|text| tomet_lsp::format_edits(text))
                        .unwrap_or_default();
                    let result = serde_json::to_value(edits)?;
                    connection
                        .sender
                        .send(Message::Response(Response::new_ok(req.id, result)))?;
                } else if req.method == HoverRequest::METHOD {
                    let params: lsp_types::HoverParams = serde_json::from_value(req.params)?;
                    let hover = documents
                        .get(&params.text_document_position_params.text_document.uri)
                        .and_then(|text| {
                            tomet_lsp::hover_for(
                                text,
                                params.text_document_position_params.position,
                            )
                        });
                    let result = serde_json::to_value(hover)?;
                    connection
                        .sender
                        .send(Message::Response(Response::new_ok(req.id, result)))?;
                } else if req.method == DocumentSymbolRequest::METHOD {
                    let params: lsp_types::DocumentSymbolParams =
                        serde_json::from_value(req.params)?;
                    let symbols = documents
                        .get(&params.text_document.uri)
                        .map(|text| tomet_lsp::document_symbols_for(text))
                        .unwrap_or_default();
                    let result = serde_json::to_value(symbols)?;
                    connection
                        .sender
                        .send(Message::Response(Response::new_ok(req.id, result)))?;
                } else if req.method == GotoDefinition::METHOD {
                    let params: lsp_types::GotoDefinitionParams =
                        serde_json::from_value(req.params)?;
                    let uri = params
                        .text_document_position_params
                        .text_document
                        .uri
                        .clone();
                    let def = documents.get(&uri).and_then(|text| {
                        tomet_lsp::definition_for(
                            text,
                            params.text_document_position_params.position,
                            &uri,
                        )
                    });
                    let result = serde_json::to_value(def)?;
                    connection
                        .sender
                        .send(Message::Response(Response::new_ok(req.id, result)))?;
                } else if req.method == Completion::METHOD {
                    let params: lsp_types::CompletionParams = serde_json::from_value(req.params)?;
                    let items = documents
                        .get(&params.text_document_position.text_document.uri)
                        .map(|text| {
                            tomet_lsp::completions_for(
                                text,
                                params.text_document_position.position,
                            )
                        })
                        .unwrap_or_default();
                    let result = serde_json::to_value(items)?;
                    connection
                        .sender
                        .send(Message::Response(Response::new_ok(req.id, result)))?;
                }
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
    let diagnostics = tomet_lsp::diagnostics_for(text);
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
