use aidl_parser::ast;
use anyhow::{Context, Result};

use crate::indexing;
use crate::state::GlobalState;
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
    // TODO: check indexing state

    let show_all_symbols = params.query.contains('#');
    let filter: String = params
        .query
        .chars()
        .filter(|&c| c != '#' && c != '*')
        .collect();

    let key_and_items: Vec<(&lsp_types::Url, &ast::Item)> = global_state
        .file_results
        .iter()
        .filter_map(|(k, fr)| fr.file.as_ref().map(|f| (k, f)))
        .map(|(k, f)| (k, &f.item))
        .collect();

    let type_symbols = key_and_items.iter().filter_map(|(uri, item)| {
        if !item
            .get_name()
            .to_lowercase()
            .contains(&filter.to_lowercase())
        {
            return None;
        }

        Some(utils::item_to_lsp_symbol_info(item, (*uri).clone()))
    });

    let mut all_symbols = if show_all_symbols {
        type_symbols
            .chain(key_and_items.iter().flat_map(|(uri, item)| {
                match item {
                    ast::Item::Interface(i) => i
                        .elements
                        .iter()
                        .map(|el| utils::interface_element_to_lsp_symbol_info(el, (*uri).clone()))
                        .collect::<Vec<_>>(),
                    ast::Item::Parcelable(p) => p
                        .members
                        .iter()
                        .map(|m| utils::member_to_lsp_symbol_info(m, (*uri).clone()))
                        .collect::<Vec<_>>(),
                    ast::Item::Enum(e) => e
                        .elements
                        .iter()
                        .map(|el| utils::enum_element_to_lsp_symbol_info(el, (*uri).clone()))
                        .collect::<Vec<_>>(),
                }
            }))
            .collect::<Vec<_>>()
    } else {
        type_symbols.collect::<Vec<_>>()
    };

    all_symbols.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(Some(all_symbols))
}

pub fn handle_document_symbol(
    global_state: &mut GlobalState,
    params: lsp_types::DocumentSymbolParams,
) -> Result<Option<lsp_types::DocumentSymbolResponse>> {
    // TODO: check indexing state

    let file_results = global_state
        .file_results
        .get(&params.text_document.uri)
        .context("File not found")?;

    let file = match &file_results.file {
        Some(f) => f,
        None => return Ok(Some(lsp_types::DocumentSymbolResponse::Nested(Vec::new()))),
    };

    #[allow(deprecated)]
    let root_symbol = match &file.item {
        ast::Item::Interface(interface) => {
            let element_symbols = interface
                .elements
                .iter()
                .map(utils::interface_element_to_lsp_document_symbol)
                .collect();

            utils::interface_to_lsp_document_symbol(interface, element_symbols)
        }
        ast::Item::Parcelable(parcelable) => {
            let member_symbols = parcelable
                .members
                .iter()
                .map(utils::member_to_lsp_document_symbol)
                .collect();

            utils::parcelable_to_lsp_document_symbol(parcelable, member_symbols)
        }
        ast::Item::Enum(enum_) => {
            let element_symbols = enum_
                .elements
                .iter()
                .map(utils::enum_element_to_lsp_document_symbol)
                .collect();

            utils::enum_to_lsp_document_symbol(enum_, element_symbols)
        }
    };

    Ok(Some(lsp_types::DocumentSymbolResponse::Nested(Vec::from(
        [root_symbol],
    ))))
}

pub fn handle_hover(
    global_state: &mut GlobalState,
    params: lsp_types::HoverParams,
) -> Result<Option<lsp_types::Hover>> {
    // TODO: check indexing state

    let file_results = global_state
        .file_results
        .get(&params.text_document_position_params.text_document.uri)
        .context("File not found")?;

    let file = match &file_results.file {
        Some(f) => f,
        None => return Ok(None),
    };

    struct FindSymbolVisitor {
        hover: Option<lsp_types::Hover>,
    }

    impl utils::FindSymbolVisitor for FindSymbolVisitor {
        fn visit_interface(&mut self, i: &ast::Interface) {
            let markdown = lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: ["# Header", "Some text", "```aidl", "someCode();", "```"].join("\n"),
            };

            self.hover = Some(lsp_types::Hover {
                contents: lsp_types::HoverContents::Markup(markdown),
                range: Some(utils::to_lsp_range(&i.symbol_range)),
            });
        }

        fn visit_parcelable(&mut self, _: &ast::Parcelable) {}

        fn visit_enum(&mut self, _: &ast::Enum) {}

        fn visit_type(&mut self, _: &ast::Type) {}
    }

    let mut visitor = FindSymbolVisitor { hover: None };
    utils::visit(
        file,
        &mut visitor,
        utils::from_lsp_position(&params.text_document_position_params.position),
    );

    Ok(visitor.hover)
}

pub fn handle_goto_definition(
    global_state: &mut GlobalState,
    params: lsp_types::GotoDefinitionParams,
) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
    // TODO: check indexing state

    let file_results = global_state
        .file_results
        .get(&params.text_document_position_params.text_document.uri)
        .context("File not found")?;

    let file = match &file_results.file {
        Some(f) => f,
        None => return Ok(None),
    };

    let pos = utils::from_lsp_position(&params.text_document_position_params.position);

    for import in &file.imports {
        if utils::range_contains(&import.symbol_range, pos) {
            return if let Some(link) = utils::get_target_link(
                global_state,
                &import.symbol_range,
                &import.get_qualified_name(),
            ) {
                Ok(Some(lsp_types::GotoDefinitionResponse::Link(Vec::from([
                    link,
                ]))))
            } else {
                Ok(None)
            };
        }
    }

    struct FindSymbolVisitor<'t> {
        link: Option<lsp_types::LocationLink>,
        global_state: &'t GlobalState,
    }

    impl<'t> utils::FindSymbolVisitor for FindSymbolVisitor<'t> {
        fn visit_type(&mut self, t: &ast::Type) {
            if let Some(link) = t
                .definition
                .as_ref()
                .map(|def| utils::get_target_link(self.global_state, &t.symbol_range, def))
                .flatten()
            {
                self.link = Some(link);
            }
        }
    }

    let mut visitor = FindSymbolVisitor {
        link: None,
        global_state,
    };

    utils::visit(file, &mut visitor, pos);

    Ok(visitor
        .link
        .map(|l| lsp_types::GotoDefinitionResponse::Link(Vec::from([l]))))
}

pub fn handle_did_change_text_document(
    global_state: &mut GlobalState,
    params: lsp_types::DidChangeTextDocumentParams,
) -> Result<()> {
    // TODO: check indexing state

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
    params: lsp_types::DidOpenTextDocumentParams,
) -> Result<()> {
    indexing::update_file(global_state, &params.text_document.uri)?;

    Ok(())
}

pub fn handle_did_save_text_document(
    global_state: &mut GlobalState,
    params: lsp_types::DidSaveTextDocumentParams,
) -> Result<()> {
    indexing::update_file(global_state, &params.text_document.uri)?;

    Ok(())
}
