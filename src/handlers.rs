use aidl_parser::symbol::Symbol;
use aidl_parser::traverse::SymbolFilter;
use aidl_parser::{ast, traverse};
use anyhow::Result;

use crate::indexing;
use crate::state::{GlobalState, IndexingState};
use crate::utils;

pub fn handle_initialized(
    _global_state: &mut GlobalState,
    _params: lsp_types::InitializedParams,
) -> Result<()> {
    Ok(())
}

pub fn handle_workspace_symbol(
    global_state: &mut GlobalState,
    params: lsp_types::WorkspaceSymbolParams,
) -> Result<Option<Vec<lsp_types::SymbolInformation>>> {
    if global_state.indexing_state != IndexingState::Indexed {
        anyhow::bail!("Cannot handle workspace symbol request: workspace has not been indexed!");
    }

    let show_all_symbols = params.query.contains('#');
    let name_filter: String = params
        .query
        .chars()
        .filter(|&c| c != '#' && c != '*')
        .collect();

    let symbol_filter = if show_all_symbols {
        SymbolFilter::All
    } else {
        SymbolFilter::ItemsOnly
    };

    // Collect all symbols by iterating over files and walking into symbols
    let mut symbols = Vec::new();
    global_state
        .file_results
        .iter()
        .filter_map(|(path, fr)| fr.ast.as_ref().map(|fr| (path, fr)))
        .filter_map(|(path, ast)| utils::path_to_uri(path).ok().map(|uri| (uri, ast)))
        .for_each(|(uri, ast)| {
            traverse::walk_symbols(ast, symbol_filter, |symbol| {
                // Filter by name
                match symbol.get_name() {
                    Some(n) if n.to_lowercase().contains(&name_filter) => (),
                    _ => return,
                };

                // Convert to LSP symbol and add it to the list
                if let Some(symbol) = utils::to_lsp_symbol_info(&symbol, uri.clone()) {
                    symbols.push(symbol);
                }
            });
        });

    // Sort symbols by name
    symbols.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(Some(symbols))
}

pub fn handle_document_symbol(
    global_state: &mut GlobalState,
    params: lsp_types::DocumentSymbolParams,
) -> Result<Option<lsp_types::DocumentSymbolResponse>> {
    if global_state.indexing_state != IndexingState::Indexed {
        anyhow::bail!("Cannot handle document symbol request: workspace has not been indexed!");
    }

    let path = utils::uri_to_path(&params.text_document.uri)?;
    let file_results = utils::get_file_results(global_state, &path)?;

    let ast = match &file_results.ast {
        Some(f) => f,
        None => return Ok(Some(lsp_types::DocumentSymbolResponse::Nested(Vec::new()))),
    };

    // Collect all symbols by walking into symbols
    let mut root_symbol: Option<lsp_types::DocumentSymbol> = None;

    enum SymbolDef {
        TopLevel,
        Child,
        None,
    }

    traverse::walk_symbols(ast, SymbolFilter::ItemsAndItemElements, |symbol| {
        // Convert to LSP symbol and add it to the tree
        if let Some(doc_symbol) = utils::to_lsp_doc_symbol(&symbol) {
            let symbol_def = match symbol {
                Symbol::Package(..) => SymbolDef::TopLevel,
                Symbol::Import(..) => SymbolDef::TopLevel,
                Symbol::Interface(..) => SymbolDef::TopLevel,
                Symbol::Parcelable(..) => SymbolDef::TopLevel,
                Symbol::Enum(..) => SymbolDef::TopLevel,
                Symbol::Method(..) => SymbolDef::Child,
                Symbol::Arg(..) => SymbolDef::None,
                Symbol::Const(..) => SymbolDef::Child,
                Symbol::Field(..) => SymbolDef::Child,
                Symbol::EnumElement(..) => SymbolDef::Child,
                Symbol::Type(..) => SymbolDef::None,
            };

            match symbol_def {
                SymbolDef::TopLevel => {
                    if let Some(root_symbol) = &root_symbol {
                        tracing::warn!(
                            "Multiple root symbols found: {} and {}",
                            root_symbol.name,
                            doc_symbol.name
                        );
                    } else {
                        // Set as root symbol
                        root_symbol = Some(doc_symbol);
                    }
                }
                SymbolDef::Child => {
                    if let Some(parent) = &mut root_symbol {
                        if let Some(children) = parent.children.as_mut() {
                            // Append to the list of children
                            children.push(doc_symbol);
                        } else {
                            // First child
                            parent.children = Some(Vec::from([doc_symbol]));
                        }
                    } else {
                        tracing::warn!("No parent symbol found for doc symbol {}", doc_symbol.name);
                    }
                }
                SymbolDef::None => (),
            };
        }
    });

    let symbols = match root_symbol {
        Some(rs) => Vec::from([rs]),
        None => Vec::new(),
    };

    tracing::info!("Document symbols = {:?}", symbols);
    Ok(Some(lsp_types::DocumentSymbolResponse::Nested(symbols)))
}

pub fn handle_hover(
    global_state: &mut GlobalState,
    params: lsp_types::HoverParams,
) -> Result<Option<lsp_types::Hover>> {
    if global_state.indexing_state != IndexingState::Indexed {
        anyhow::bail!("Cannot handle hover request: workspace has not been indexed!");
    }

    let path = utils::uri_to_path(&params.text_document_position_params.text_document.uri)?;
    let file_results = utils::get_file_results(global_state, &path)?;

    let ast = match &file_results.ast {
        Some(f) => f,
        None => return Ok(None),
    };

    let target_line_col = utils::from_lsp_position(&params.text_document_position_params.position);
    let hover =
        traverse::find_symbol_at_line_col(ast, traverse::SymbolFilter::All, target_line_col).map(
            |smb| {
                let signature: Option<String> = if let Symbol::Type(ast::Type {
                    kind: ast::TypeKind::Resolved(qualified_name, Some(item_kind)),
                    ..
                }) = smb
                {
                    let name = qualified_name.split('.').last().unwrap_or("");
                    Some(match item_kind {
                        ast::ItemKind::Interface => format!("interface {}", name),
                        ast::ItemKind::Parcelable => format!("parcelable {}", name),
                        ast::ItemKind::Enum => format!("enum {}", name),
                    })
                } else {
                    None
                };

                let signature = signature.unwrap_or_else(|| smb.get_signature());

                let markdown = lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: [
                        &smb.get_qualified_name().unwrap_or_default(),
                        "```aidl",
                        &signature,
                        "```",
                    ]
                    .join("\n"),
                };

                lsp_types::Hover {
                    contents: lsp_types::HoverContents::Markup(markdown),
                    range: Some(utils::to_lsp_range(smb.get_range())),
                }
            },
        );

    Ok(hover)
}

pub fn handle_goto_definition(
    global_state: &mut GlobalState,
    params: lsp_types::GotoDefinitionParams,
) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
    if global_state.indexing_state != IndexingState::Indexed {
        anyhow::bail!("Cannot handle goto definition request: workspace has not been indexed!");
    }

    let path = utils::uri_to_path(&params.text_document_position_params.text_document.uri)?;
    let file_results = utils::get_file_results(global_state, &path)?;

    let file = match &file_results.ast {
        Some(f) => f,
        None => return Ok(None),
    };

    let pos = utils::from_lsp_position(&params.text_document_position_params.position);

    let link = traverse::find_symbol_at_line_col(file, SymbolFilter::All, pos)
        .map(|symbol| {
            let key_and_range = match symbol {
                Symbol::Import(i) => Some((i.get_qualified_name(), &i.symbol_range)),
                Symbol::Type(
                    t
                    @
                    ast::Type {
                        kind: ast::TypeKind::Resolved(qualified_name, _),
                        ..
                    },
                ) => Some((qualified_name.clone(), &t.symbol_range)),
                _ => None,
            };

            key_and_range
                .map(|(key, range)| utils::get_target_link(global_state, range, &key))
                .flatten()
        })
        .flatten();

    let response = link.map(|l| lsp_types::GotoDefinitionResponse::Link(Vec::from([l])));

    Ok(response)
}

pub fn handle_did_change_text_document(
    global_state: &mut GlobalState,
    params: lsp_types::DidChangeTextDocumentParams,
) -> Result<()> {
    if global_state.indexing_state != IndexingState::Indexed {
        anyhow::bail!(
            "Cannot handle did change text document notification: workspace has not been indexed!"
        );
    }

    if params.content_changes.len() != 1 {
        anyhow::bail!(
            "Unsupported content change with length {}",
            params.content_changes.len()
        );
    }

    let content_change = &params.content_changes[0];
    if content_change.range.is_some() {
        anyhow::bail!("Unexpected range in content change: only full change can be provided!");
    }

    indexing::update_content(
        global_state,
        &params.text_document.uri,
        &content_change.text,
    )?;

    Ok(())
}

pub fn handle_did_open_text_document(
    global_state: &mut GlobalState,
    _params: lsp_types::DidOpenTextDocumentParams,
) -> Result<()> {
    if global_state.indexing_state != IndexingState::Indexed {
        anyhow::bail!(
            "Cannot handle did open text document notification: workspace has not been indexed!"
        );
    }

    Ok(())
}

pub fn handle_did_save_text_document(
    global_state: &mut GlobalState,
    params: lsp_types::DidSaveTextDocumentParams,
) -> Result<()> {
    if global_state.indexing_state != IndexingState::Indexed {
        anyhow::bail!(
            "Cannot handle did save text document notification: workspace has not been indexed!"
        );
    }

    indexing::update_file(global_state, &params.text_document.uri)?;

    Ok(())
}
