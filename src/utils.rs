use aidl_parser::ast;

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

pub fn range_contains(range: &ast::Range, line_col: (usize, usize)) -> bool {
    if range.start.line_col.0 > line_col.0 {
        return false;
    }

    if range.start.line_col.0 == line_col.0 && range.start.line_col.1 > line_col.1 {
        return false;
    }

    if range.end.line_col.0 < line_col.0 {
        return false;
    }

    if range.end.line_col.0 == line_col.0 && range.end.line_col.1 < line_col.1 {
        return false;
    }

    true
}

pub trait FindSymbolVisitor {
    fn visit_interface(&mut self, _: &ast::Interface) {}
    fn visit_parcelable(&mut self, _: &ast::Parcelable) {}
    fn visit_enum(&mut self, _: &ast::Enum) {}
    fn visit_type(&mut self, _: &ast::Type) {}
}

// TODO: optimize it to skip elements when the full range is before line_col
#[allow(clippy::needless_return)]
pub fn visit(file: &ast::File, visitor: &mut impl FindSymbolVisitor, line_col: (usize, usize)) {
    let mut visit_type_helper = |t: &ast::Type| {
        // Generic types (on purpose before the main type!)
        for t in &t.generic_types {
            if range_contains(&t.symbol_range, line_col) {
                visitor.visit_type(t);
                return true;
            }
        }

        // Main type
        if range_contains(&t.symbol_range, line_col) {
            visitor.visit_type(t);
            return true;
        }

        false
    };

    match &file.item {
        ast::Item::Interface(ref interface) => {
            if range_contains(&interface.symbol_range, line_col) {
                visitor.visit_interface(interface);
                return;
            }

            for el in &interface.elements {
                match el {
                    ast::InterfaceElement::Method(m) => {
                        if visit_type_helper(&m.return_type) {
                            return;
                        }
                        for arg in &m.args {
                            if visit_type_helper(&arg.arg_type) {
                                return;
                            }
                        }
                    }
                    ast::InterfaceElement::Const(c) => {
                        if visit_type_helper(&c.const_type) {
                            return;
                        }
                    }
                }
            }
        }
        ast::Item::Parcelable(ref parcelable) => {
            if range_contains(&parcelable.symbol_range, line_col) {
                visitor.visit_parcelable(parcelable);
                return;
            }

            for m in &parcelable.members {
                if visit_type_helper(&m.member_type) {
                    return;
                }
            }
        }

        ast::Item::Enum(enum_) => {
            if range_contains(&enum_.symbol_range, line_col) {
                visitor.visit_enum(enum_);
                return;
            }
        }
    }
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
        .map(|fr| fr.file.as_ref().map(|file| (&fr.id, file)))
        .flatten()
        .map(|(uri, file)| lsp_types::LocationLink {
            origin_selection_range: Some(to_lsp_range(origin_range)),
            target_uri: uri.clone(),
            target_range: to_lsp_range(file.item.get_symbol_range()),
            target_selection_range: to_lsp_range(file.item.get_full_range()),
        })
}

pub fn interface_to_lsp_document_symbol(
    interface: &ast::Interface,
    children: Vec<lsp_types::DocumentSymbol>,
) -> lsp_types::DocumentSymbol {
    #[allow(deprecated)]
    lsp_types::DocumentSymbol {
        name: interface.name.clone(),
        detail: Some(format!("interface {}", interface.name)),
        kind: lsp_types::SymbolKind::INTERFACE,
        tags: None,
        deprecated: None,
        range: to_lsp_range(&interface.full_range),
        selection_range: to_lsp_range(&interface.symbol_range),
        children: Some(children),
    }
}

pub fn parcelable_to_lsp_document_symbol(
    parcelable: &ast::Parcelable,
    children: Vec<lsp_types::DocumentSymbol>,
) -> lsp_types::DocumentSymbol {
    #[allow(deprecated)]
    lsp_types::DocumentSymbol {
        name: parcelable.name.clone(),
        detail: Some(format!("parcelable {}", parcelable.name)),
        kind: lsp_types::SymbolKind::STRUCT,
        tags: None,
        deprecated: None,
        range: to_lsp_range(&parcelable.full_range),
        selection_range: to_lsp_range(&parcelable.symbol_range),
        children: Some(children),
    }
}

pub fn enum_to_lsp_document_symbol(
    enum_: &ast::Enum,
    children: Vec<lsp_types::DocumentSymbol>,
) -> lsp_types::DocumentSymbol {
    #[allow(deprecated)]
    lsp_types::DocumentSymbol {
        name: enum_.name.clone(),
        detail: Some(format!("enum {}", enum_.name)),
        kind: lsp_types::SymbolKind::ENUM,
        tags: None,
        deprecated: None,
        range: to_lsp_range(&enum_.full_range),
        selection_range: to_lsp_range(&enum_.symbol_range),
        children: Some(children),
    }
}

pub fn interface_element_to_lsp_document_symbol(
    element: &ast::InterfaceElement,
) -> lsp_types::DocumentSymbol {
    match element {
        #[allow(deprecated)]
        ast::InterfaceElement::Const(c) => lsp_types::DocumentSymbol {
            name: c.name.clone(),
            detail: Some(format!("{} = {}", c.name, c.value)),
            kind: lsp_types::SymbolKind::CONSTANT,
            tags: None,
            deprecated: None,
            range: to_lsp_range(&c.full_range),
            selection_range: to_lsp_range(&c.symbol_range),
            children: None,
        },
        #[allow(deprecated)]
        ast::InterfaceElement::Method(m) => lsp_types::DocumentSymbol {
            name: m.name.clone(),
            detail: Some(m.get_signature()),
            kind: lsp_types::SymbolKind::METHOD,
            tags: None,
            deprecated: None,
            range: to_lsp_range(&m.full_range),
            selection_range: to_lsp_range(&m.symbol_range),
            children: None,
        },
    }
}

pub fn enum_element_to_lsp_document_symbol(
    enum_element: &ast::EnumElement,
) -> lsp_types::DocumentSymbol {
    #[allow(deprecated)]
    lsp_types::DocumentSymbol {
        name: enum_element.name.clone(),
        detail: enum_element
            .value
            .as_ref()
            .map(|v| format!("{} = {}", enum_element.name, v)),
        kind: lsp_types::SymbolKind::ENUM_MEMBER,
        tags: None,
        deprecated: None,
        range: to_lsp_range(&enum_element.full_range),
        selection_range: to_lsp_range(&enum_element.symbol_range),
        children: None,
    }
}

pub fn item_to_lsp_symbol_info(
    item: &ast::Item,
    uri: lsp_types::Url,
) -> lsp_types::SymbolInformation {
    let kind = match item {
        ast::Item::Interface(_) => lsp_types::SymbolKind::INTERFACE,
        ast::Item::Parcelable(_) => lsp_types::SymbolKind::STRUCT,
        ast::Item::Enum(_) => lsp_types::SymbolKind::ENUM,
    };

    let location = lsp_types::Location {
        uri,
        range: to_lsp_range(item.get_symbol_range()),
    };

    #[allow(deprecated)]
    lsp_types::SymbolInformation {
        name: item.get_name().to_owned(),
        kind,
        tags: None,
        deprecated: None,
        location,
        container_name: None,
    }
}

pub fn interface_element_to_lsp_symbol_info(
    element: &ast::InterfaceElement,
    uri: lsp_types::Url,
) -> lsp_types::SymbolInformation {
    let location = lsp_types::Location {
        uri,
        range: to_lsp_range(element.get_symbol_range()),
    };

    #[allow(deprecated)]
    lsp_types::SymbolInformation {
        name: element.get_name().to_owned(),
        kind: match element {
            ast::InterfaceElement::Const(_) => lsp_types::SymbolKind::CONSTANT,
            ast::InterfaceElement::Method(_) => lsp_types::SymbolKind::METHOD,
        },
        tags: None,
        deprecated: None,
        location,
        container_name: None,
    }
}

pub fn member_to_lsp_document_symbol(member: &ast::Member) -> lsp_types::DocumentSymbol {
    #[allow(deprecated)]
    lsp_types::DocumentSymbol {
        name: member.name.clone(),
        detail: Some(member.get_signature()),
        kind: lsp_types::SymbolKind::FIELD,
        tags: None,
        deprecated: None,
        range: to_lsp_range(&member.full_range),
        selection_range: to_lsp_range(&member.symbol_range),
        children: None,
    }
}

pub fn member_to_lsp_symbol_info(
    member: &ast::Member,
    uri: lsp_types::Url,
) -> lsp_types::SymbolInformation {
    let location = lsp_types::Location {
        uri,
        range: to_lsp_range(&member.symbol_range),
    };

    #[allow(deprecated)]
    lsp_types::SymbolInformation {
        name: member.name.clone(),
        kind: lsp_types::SymbolKind::FIELD,
        tags: None,
        deprecated: None,
        location,
        container_name: None,
    }
}

pub fn enum_element_to_lsp_symbol_info(
    enum_element: &ast::EnumElement,
    uri: lsp_types::Url,
) -> lsp_types::SymbolInformation {
    let location = lsp_types::Location {
        uri,
        range: to_lsp_range(&enum_element.symbol_range),
    };

    #[allow(deprecated)]
    lsp_types::SymbolInformation {
        name: enum_element.name.clone(),
        kind: lsp_types::SymbolKind::ENUM_MEMBER,
        tags: None,
        deprecated: None,
        location,
        container_name: None,
    }
}
