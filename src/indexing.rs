use anyhow::{Context, Result};
use lsp_types::notification::Notification;
use std::{fs::File, io::Read};
use walkdir::WalkDir;

use crate::{state::GlobalState, state::IndexingState, utils};

pub fn index(global_state: &mut GlobalState) -> Result<()> {
    if global_state.indexing_state == IndexingState::Indexing {
        tracing::warn!("Cannot index: already indexing!");
        return Ok(());
    }

    global_state.indexing_state = IndexingState::Indexing;

    match do_index(global_state) {
        Ok(()) => {
            global_state.indexing_state = IndexingState::Indexed;
        }
        Err(e) => {
            global_state.indexing_state = IndexingState::Error;
            anyhow::bail!(e);
        }
    }

    Ok(())
}

fn do_index(global_state: &mut GlobalState) -> Result<()> {
    let path = global_state
        .root_path
        .as_ref()
        .context("No root path set")?;

    let mut aidl_file_entries = WalkDir::new(path)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "aidl")
                .unwrap_or(false)
        });

    aidl_file_entries.try_for_each(|e| {
        let path = std::fs::canonicalize(e.path())?;

        let mut file = File::open(&path)?;
        let mut buffer = String::new();
        file.read_to_string(&mut buffer)?;

        tracing::debug!("Parsing {:?}", e.path());
        let uri = lsp_types::Url::from_file_path(&path).unwrap();
        global_state.parser.add_content(uri, &buffer);

        Ok(()) as Result<()>
    })?;

    global_state.file_results = global_state.parser.validate();
    global_state.items_by_key = global_state
        .file_results
        .iter()
        .filter_map(|(id, fr)| fr.ast.as_ref().map(|f| (f.get_key(), id.clone())))
        .collect();

    notify_diagnostics(global_state);

    Ok(())
}

pub fn update_file(global_state: &mut GlobalState, uri: &lsp_types::Url) -> Result<()> {
    let path = uri
        .to_file_path()
        .map_err(|_| anyhow::format_err!("Invalid root path (not a file path): {}", uri))?;

    let mut file = std::fs::File::open(path)?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;

    update_content(global_state, uri, &buffer)?;

    Ok(())
}

pub fn update_content(
    global_state: &mut GlobalState,
    uri: &lsp_types::Url,
    content: &str,
) -> Result<()> {
    let parser = &mut global_state.parser;

    parser.add_content(uri.clone(), content);
    global_state.file_results = parser.validate();
    global_state.items_by_key = global_state
        .file_results
        .iter()
        .filter_map(|(id, fr)| fr.ast.as_ref().map(|f| (f.get_key(), id.clone())))
        .collect();

    notify_diagnostics(global_state);
    notify_diagnostics(global_state);

    Ok(())
}

fn notify_diagnostics(global_state: &GlobalState) {
    for res in global_state.file_results.values() {
        let uri = &res.id;

        let diagnostics = res
            .diagnostics
            .iter()
            .map(|d| {
                let main_location = lsp_types::Location {
                    uri: res.id.clone(),
                    range: utils::to_lsp_range(&d.range),
                };

                let mut related_infos = Vec::new();

                // Note: do not add the context message which is redundant with the main message
                //if let Some(ctx_msg) = &d.context_message {
                //    related_infos.push(lsp_types::DiagnosticRelatedInformation {
                //        location: main_location.clone(),
                //        message: ctx_msg.clone(),
                //    });
                //}

                if let Some(hint) = &d.hint {
                    related_infos.push(lsp_types::DiagnosticRelatedInformation {
                        location: main_location,
                        message: hint.clone(),
                    });
                }

                let related_infos = related_infos
                    .into_iter()
                    .chain(d.related_infos.iter().map(|ri| {
                        lsp_types::DiagnosticRelatedInformation {
                            location: lsp_types::Location {
                                uri: res.id.clone(),
                                range: utils::to_lsp_range(&ri.range),
                            },
                            message: ri.message.clone(),
                        }
                    }))
                    .collect();

                lsp_types::Diagnostic::new(
                    utils::to_lsp_range(&d.range),
                    Some(match d.kind {
                        aidl_parser::diagnostic::DiagnosticKind::Error => {
                            lsp_types::DiagnosticSeverity::ERROR
                        }
                        aidl_parser::diagnostic::DiagnosticKind::Warning => {
                            lsp_types::DiagnosticSeverity::WARNING
                        }
                    }),
                    Some(lsp_types::NumberOrString::String("aidl".to_owned())),
                    None,
                    d.message.clone(),
                    Some(related_infos),
                    None,
                )
            })
            .collect();

        let notif = lsp_server::Notification::new(
            lsp_types::notification::PublishDiagnostics::METHOD.to_owned(),
            lsp_types::PublishDiagnosticsParams {
                uri: uri.clone(),
                diagnostics,
                version: None,
            },
        );
        global_state
            .sender
            .send(lsp_server::Message::Notification(notif))
            .unwrap();
    }
}
