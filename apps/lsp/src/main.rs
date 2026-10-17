use std::collections::HashMap;
use std::time::{Duration, Instant};

use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
    DidSaveTextDocument, LogMessage, Notification as _, PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, FoldingRangeRequest, Formatting, GotoDefinition,
    HoverRequest, RegisterCapability, Request as _,
};
use lsp_types::{
    CompletionOptions, DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher,
    FoldingRangeProviderCapability, GlobPattern, HoverProviderCapability, InitializeParams,
    LogMessageParams, MessageType, OneOf, PublishDiagnosticsParams, Registration,
    RegistrationParams, SaveOptions, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Uri,
};
use tomet_lsp::{Debounce, Workspace};

fn main() -> anyhow::Result<()> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        // Saves are told to the server too: a saved file is a changed file on
        // the disk, which a document that includes it may read.
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(false),
                })),
                ..Default::default()
            },
        )),
        document_formatting_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(vec![
                "<".to_string(),
                "@".to_string(),
                "$".to_string(),
                ":".to_string(),
                "/".to_string(),
                ".".to_string(),
            ]),
            work_done_progress_options: Default::default(),
            all_commit_characters: None,
            completion_item: None,
        }),
        ..Default::default()
    };
    let init_params = connection.initialize(serde_json::to_value(capabilities)?)?;
    let init_params: InitializeParams = serde_json::from_value(init_params)?;

    // Documents read other files (an `include`, a vocabulary, the config), so
    // a change to any `.tmt` file on the disk is worth hearing about -- from
    // the editor's own saves, and from git, a formatter, another program.
    let can_watch = init_params
        .capabilities
        .workspace
        .and_then(|w| w.did_change_watched_files)
        .and_then(|w| w.dynamic_registration)
        .unwrap_or(false);
    if can_watch {
        register_file_watcher(&connection)?;
    }

    main_loop(connection)?;
    io_threads.join()?;
    Ok(())
}

fn main_loop(connection: Connection) -> anyhow::Result<()> {
    // `Uri` holds `Cell`s inside, which is what clippy objects to, but its
    // `Hash` and `Eq` compare the text alone, so it is safe as a key.
    #[allow(clippy::mutable_key_type)]
    let mut documents: HashMap<Uri, String> = HashMap::new();
    let mut workspace = Workspace::new();
    // What the vault said about each open document the last time it was asked,
    // published together with what the text alone says.
    #[allow(clippy::mutable_key_type)]
    let mut from_vault: HashMap<Uri, Vec<lsp_types::Diagnostic>> = HashMap::new();
    let mut debounce = Debounce::new(VAULT_DIAGNOSTICS_DELAY);
    loop {
        // Waiting for a message is also waiting for the pause that the
        // vault's diagnostics are due after.
        let received = match debounce.wait(Instant::now()) {
            None => match connection.receiver.recv() {
                Ok(msg) => Some(msg),
                Err(_) => break,
            },
            Some(wait) => match connection.receiver.recv_timeout(wait) {
                Ok(msg) => Some(msg),
                Err(e) if e.is_timeout() => None,
                Err(_) => break,
            },
        };
        if debounce.due(Instant::now()) {
            refresh(&connection, &documents, &mut workspace, &mut from_vault)?;
        }
        // A vault found something wrong with its own setup while it was built
        // for the messages so far, or for the refresh.
        log_notices(&connection, &mut workspace)?;
        let Some(msg) = received else {
            continue;
        };
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
                        .map(|text| tomet_lsp::format_edits(text, Some(&params.text_document.uri)))
                        .unwrap_or_default();
                    let result = serde_json::to_value(edits)?;
                    connection
                        .sender
                        .send(Message::Response(Response::new_ok(req.id, result)))?;
                } else if req.method == HoverRequest::METHOD {
                    let params: lsp_types::HoverParams = serde_json::from_value(req.params)?;
                    let uri = &params.text_document_position_params.text_document.uri;
                    let position = params.text_document_position_params.position;
                    // A file in a vault is answered by its vault; anything
                    // else, from its text alone.
                    let hover = match tomet_lsp::uri_to_file_path(uri) {
                        Some(path) => workspace.hover(&path, position),
                        None => documents
                            .get(uri)
                            .and_then(|text| tomet_lsp::hover_for(text, position, Some(uri))),
                    };
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
                    let position = params.text_document_position_params.position;
                    // A file in a vault is answered by its vault, which can
                    // follow a reference into another document; anything
                    // else, from its text alone.
                    let def = match tomet_lsp::uri_to_file_path(&uri) {
                        Some(path) => workspace.definition(&path, position),
                        None => documents
                            .get(&uri)
                            .and_then(|text| tomet_lsp::definition_for(text, position, &uri)),
                    };
                    let result = serde_json::to_value(def)?;
                    connection
                        .sender
                        .send(Message::Response(Response::new_ok(req.id, result)))?;
                } else if req.method == Completion::METHOD {
                    let params: lsp_types::CompletionParams = serde_json::from_value(req.params)?;
                    let uri = &params.text_document_position.text_document.uri;
                    let position = params.text_document_position.position;
                    // A file in a vault is answered by its vault; anything
                    // else, from its text alone.
                    let items = match tomet_lsp::uri_to_file_path(uri) {
                        Some(path) => workspace.completions(&path, position, Some(uri)),
                        None => documents
                            .get(uri)
                            .map(|text| {
                                tomet_lsp::completions_for_with_uri(text, position, Some(uri))
                            })
                            .unwrap_or_default(),
                    };
                    let result = serde_json::to_value(items)?;
                    connection
                        .sender
                        .send(Message::Response(Response::new_ok(req.id, result)))?;
                } else if req.method == FoldingRangeRequest::METHOD {
                    let params: lsp_types::FoldingRangeParams = serde_json::from_value(req.params)?;
                    let ranges = documents
                        .get(&params.text_document.uri)
                        .map(|text| tomet_lsp::folding_ranges_for(text))
                        .unwrap_or_default();
                    let result = serde_json::to_value(ranges)?;
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
                    if let Some(path) = tomet_lsp::uri_to_file_path(&uri) {
                        workspace.set_buffer(path, params.text_document.text.clone());
                        debounce.touch(Instant::now());
                    }
                    publish(
                        &connection,
                        uri.clone(),
                        &params.text_document.text,
                        from_vault.get(&uri),
                    )?;
                }
                DidChangeTextDocument::METHOD => {
                    let params: lsp_types::DidChangeTextDocumentParams =
                        serde_json::from_value(note.params)?;
                    let uri = params.text_document.uri;
                    if let Some(change) = params.content_changes.into_iter().next_back() {
                        documents.insert(uri.clone(), change.text.clone());
                        if let Some(path) = tomet_lsp::uri_to_file_path(&uri) {
                            workspace.set_buffer(path, change.text.clone());
                            debounce.touch(Instant::now());
                        }
                        // The text alone is checked now; the vault, once the
                        // typing pauses. Until then the vault's last answer
                        // stands.
                        publish(&connection, uri.clone(), &change.text, from_vault.get(&uri))?;
                    }
                }
                DidCloseTextDocument::METHOD => {
                    let params: lsp_types::DidCloseTextDocumentParams =
                        serde_json::from_value(note.params)?;
                    let uri = params.text_document.uri;
                    documents.remove(&uri);
                    from_vault.remove(&uri);
                    if let Some(path) = tomet_lsp::uri_to_file_path(&uri) {
                        workspace.close(&path);
                        // What was open and read it may read the disk's now.
                        debounce.touch(Instant::now());
                    }
                    publish_diagnostics(&connection, uri, Vec::new())?;
                }
                // A file was saved, or changed behind the editor's back: a
                // config, a vocabulary, a document that others include.
                DidSaveTextDocument::METHOD | DidChangeWatchedFiles::METHOD => {
                    workspace.files_changed();
                    debounce.touch(Instant::now());
                }
                _ => {}
            },
            Message::Response(_) => {}
        }
    }
    Ok(())
}

/// How long the typing has to pause before the vault is asked about the open
/// documents. What the text alone says (the parser, the CST validator) does
/// not wait.
const VAULT_DIAGNOSTICS_DELAY: Duration = Duration::from_millis(300);

/// What the text alone says, and `from_vault` -- what the vault said about the
/// document the last time it was asked.
fn publish(
    connection: &Connection,
    uri: Uri,
    text: &str,
    from_vault: Option<&Vec<lsp_types::Diagnostic>>,
) -> anyhow::Result<()> {
    let mut diagnostics = tomet_lsp::diagnostics_for(text);
    diagnostics.extend(from_vault.into_iter().flatten().cloned());
    publish_diagnostics(connection, uri, diagnostics)
}

/// Asks the vault about every open document again and publishes the ones it
/// now says something different about. A document nothing changed for is not
/// published again: a client redraws on every publish.
#[allow(clippy::mutable_key_type)]
fn refresh(
    connection: &Connection,
    documents: &HashMap<Uri, String>,
    workspace: &mut Workspace,
    from_vault: &mut HashMap<Uri, Vec<lsp_types::Diagnostic>>,
) -> anyhow::Result<()> {
    for (uri, text) in documents {
        let Some(path) = tomet_lsp::uri_to_file_path(uri) else {
            continue;
        };
        let now = workspace.findings_diagnostics(&path);
        let before = from_vault.get(uri).map(Vec::as_slice).unwrap_or_default();
        if now == before {
            continue;
        }
        from_vault.insert(uri.clone(), now);
        publish(connection, uri.clone(), text, from_vault.get(uri))?;
    }
    Ok(())
}

/// Says what the vaults found wrong with their own setup in the client's log.
/// It belongs to no document, so there is no line of one to underline.
fn log_notices(connection: &Connection, workspace: &mut Workspace) -> anyhow::Result<()> {
    for message in workspace.take_notices() {
        let params = LogMessageParams {
            typ: MessageType::WARNING,
            message: format!("tomet: {message}"),
        };
        connection
            .sender
            .send(Message::Notification(lsp_server::Notification::new(
                LogMessage::METHOD.to_string(),
                params,
            )))?;
    }
    Ok(())
}

/// Asks the client to tell us when a `.tmt` file changes on the disk.
fn register_file_watcher(connection: &Connection) -> anyhow::Result<()> {
    let params = RegistrationParams {
        registrations: vec![Registration {
            id: "tomet-watch-tmt".to_string(),
            method: DidChangeWatchedFiles::METHOD.to_string(),
            register_options: Some(serde_json::to_value(
                DidChangeWatchedFilesRegistrationOptions {
                    watchers: vec![FileSystemWatcher {
                        glob_pattern: GlobPattern::String("**/*.tmt".to_string()),
                        kind: None,
                    }],
                },
            )?),
        }],
    };
    connection
        .sender
        .send(Message::Request(lsp_server::Request::new(
            RequestId::from("tomet-register-watch".to_string()),
            RegisterCapability::METHOD.to_string(),
            params,
        )))?;
    Ok(())
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
