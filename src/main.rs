use anyhow::{Context, Result};
use lsp_server::{Connection, Message};
use lsp_types::{notification, request};
use tracing::metadata::LevelFilter;

mod dispatch;
mod error;
mod handlers;
mod indexing;
mod log;
mod state;
mod utils;

use dispatch::{NotificationDispatcher, RequestDispatcher};
use state::GlobalState;

fn main() -> Result<()> {
    let subscriber_builder = tracing_subscriber::fmt()
        .event_format(log::LoggerFormatter)
        .with_writer(std::io::stderr)
        .with_max_level(LevelFilter::INFO);

    subscriber_builder.init();

    tracing::info!("Starting AIDL LSP server");

    // Create the transport
    let (connection, io_threads) = Connection::stdio();

    // Server capabilities
    let server_capabilities = serde_json::to_value(&server_capabilities()).unwrap();

    tracing::info!("Initializing connection");
    let init_params = connection.initialize(server_capabilities)?;
    tracing::info!("Init params: {}", init_params);

    // Run the server and wait for the two threads to end (typically by trigger LSP Exit event).
    tracing::info!("Starting main loop");

    if let Err(e) = main_loop(connection, init_params) {
        tracing::error!("Error in main loop: {}", e);
    }

    if let Err(e) = io_threads.join() {
        tracing::warn!("Could not join IO threads: {}", e);
    }

    tracing::info!("shutting down server");
    Ok(())
}

fn main_loop(connection: Connection, init_params: serde_json::Value) -> Result<()> {
    let init_params: lsp_types::InitializeParams = serde_json::from_value(init_params).unwrap();
    let mut global_state = GlobalState::new(connection.sender.clone());

    let root_uri = init_params.root_uri.context("Missing root URI")?;
    let file_path = root_uri
        .to_file_path()
        .map_err(|_| anyhow::format_err!("Invalid root path (not a file path): {}", root_uri))?;
    tracing::info!("root path = {:?}", file_path);
    global_state.root_path = Some(file_path);

    // Indexing (TODO: incl. progress support)
    indexing::index(&mut global_state)?;

    for msg in &connection.receiver {
        tracing::trace!("got msg: {:?}", msg);

        match msg {
            Message::Request(req) => {
                RequestDispatcher {
                    global_state: &mut global_state,
                    req: Some(req),
                }
                .on::<request::WorkspaceSymbol>(handlers::handle_workspace_symbol)?
                .on::<request::DocumentSymbolRequest>(handlers::handle_document_symbol)?
                .on::<request::HoverRequest>(handlers::handle_hover)?
                .on::<request::GotoDefinition>(handlers::handle_goto_definition)?
                .finish();
            }
            Message::Response(_) => todo!(),
            Message::Notification(notif) => {
                NotificationDispatcher {
                    connection: &connection,
                    global_state: &mut global_state,
                    notif: Some(notif),
                }
                .on::<notification::Initialized>(handlers::handle_initialized)?
                .on::<notification::DidChangeTextDocument>(
                    handlers::handle_did_change_text_document,
                )?
                .on::<notification::DidOpenTextDocument>(handlers::handle_did_open_text_document)?
                .on::<notification::DidSaveTextDocument>(handlers::handle_did_save_text_document)?
                .finish();
            }
        }
    }
    Ok(())
}

fn server_capabilities() -> lsp_types::ServerCapabilities {
    use lsp_types::*;

    ServerCapabilities {
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
            lsp_types::TextDocumentSyncKind::FULL,
        )),
        document_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        workspace_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        workspace: Some(lsp_types::WorkspaceServerCapabilities {
            workspace_folders: Some(lsp_types::WorkspaceFoldersServerCapabilities {
                supported: Some(false),
                change_notifications: Some(lsp_types::OneOf::Left(false)),
            }),
            file_operations: Some(lsp_types::WorkspaceFileOperationsServerCapabilities {
                did_create: None,
                will_create: None,
                did_rename: None,
                will_rename: None,
                did_delete: None,
                will_delete: None,
            }),
        }),
        ..ServerCapabilities::default()
    }
}
