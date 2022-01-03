use aidl_parser::{ast, symbol::Symbol};
use anyhow::Result;

use crate::state::GlobalState;

pub fn to_lsp_range(r: &ast::Range) -> lsp_types::Range {
    lsp_types::Range {
        start: to_lsp_position(&r.start),
        end: to_lsp_position(&r.end),
    }
}

// Convert 1-based line_col into LSP 0-based Position
fn to_lsp_position(p: &ast::Position) -> lsp_types::Position {
    lsp_types::Position {
        line: p.line_col.0 as u32 - 1,
        character: p.line_col.1 as u32 - 1,
    }
}

// Convert 0-based Position into 1-based line_col
pub fn from_lsp_position(p: &lsp_types::Position) -> (usize, usize) {
    (p.line as usize + 1, p.character as usize + 1)
}

pub fn get_target_link(
    global_state: &GlobalState,
    origin_range: &ast::Range,
    target_item_key: ast::ItemKeyRef,
) -> Option<lsp_types::LocationLink> {
    global_state
        .items_by_key
        .get(target_item_key)
        .map(|target_uri| global_state.file_results.get(target_uri))
        .flatten()
        .map(|fr| fr.ast.as_ref().map(|file| (&fr.id, file)))
        .flatten()
        .map(|(uri, file)| lsp_types::LocationLink {
            origin_selection_range: Some(to_lsp_range(origin_range)),
            target_uri: uri.clone(),
            target_range: to_lsp_range(file.item.get_full_range()),
            target_selection_range: to_lsp_range(file.item.get_symbol_range()),
        })
}

pub fn to_lsp_symbol_info(
    symbol: &Symbol,
    uri: lsp_types::Url,
) -> Option<lsp_types::SymbolInformation> {
    let kind = match to_lsp_symbol_kind(symbol) {
        Some(k) => k,
        None => return None,
    };

    let name = match symbol.get_name() {
        Some(n) => n,
        None => return None,
    };

    let location = lsp_types::Location {
        uri,
        range: to_lsp_range(symbol.get_range()),
    };

    #[allow(deprecated)]
    Some(lsp_types::SymbolInformation {
        name,
        kind,
        tags: None,
        deprecated: None,
        location,
        container_name: None,
    })
}

pub fn to_lsp_doc_symbol(symbol: &Symbol) -> Option<lsp_types::DocumentSymbol> {
    let kind = match to_lsp_symbol_kind(symbol) {
        Some(k) => k,
        None => return None,
    };

    let name = match symbol.get_name() {
        Some(n) => n,
        None => return None,
    };

    #[allow(deprecated)]
    Some(lsp_types::DocumentSymbol {
        name,
        detail: symbol.get_details(),
        kind,
        tags: None,
        deprecated: None,
        range: to_lsp_range(symbol.get_full_range()),
        selection_range: to_lsp_range(symbol.get_range()),
        children: None,
    })
}

fn to_lsp_symbol_kind(symbol: &Symbol) -> Option<lsp_types::SymbolKind> {
    Some(match symbol {
        Symbol::Package(_) => lsp_types::SymbolKind::PACKAGE,
        Symbol::Import(_) => lsp_types::SymbolKind::PACKAGE,
        Symbol::Interface(_) => lsp_types::SymbolKind::INTERFACE,
        Symbol::Parcelable(_) => lsp_types::SymbolKind::STRUCT,
        Symbol::Enum(_) => lsp_types::SymbolKind::ENUM,
        Symbol::Method(_) => lsp_types::SymbolKind::METHOD,
        Symbol::Arg(_) => return None,
        Symbol::Const(_) => lsp_types::SymbolKind::CONSTANT,
        Symbol::Member(_) => lsp_types::SymbolKind::FIELD,
        Symbol::EnumElement(_) => lsp_types::SymbolKind::ENUM_MEMBER,
        Symbol::Type(_) => return None,
    })
}

pub fn get_file_results<'a>(
    global_state: &'a GlobalState,
    uri: &lsp_types::Url,
) -> Result<&'a aidl_parser::ParseFileResult<lsp_types::Url>> {
    let fr = global_state
        .file_results
        .get(uri)
        .ok_or_else(|| -> anyhow::Error {
            anyhow::anyhow!(
                "File not found: `{:?}`",
                uri.to_file_path()
                    .map(|pb| pb.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| String::from("<invalid local path>"))
            )
        })?;

    Ok(fr)
}
