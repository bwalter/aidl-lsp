use aidl_parser::{ast, ParseFileResult, Parser};
use std::{collections::HashMap, path::PathBuf};

pub struct GlobalState {
    pub sender: crossbeam::channel::Sender<lsp_server::Message>,
    pub root_path: Option<PathBuf>,
    pub parser: Parser<lsp_types::Url>,
    pub file_results: HashMap<lsp_types::Url, ParseFileResult<lsp_types::Url>>,
    pub items_by_key: HashMap<ast::ItemKey, lsp_types::Url>,
}

impl GlobalState {
    pub fn new(sender: crossbeam::channel::Sender<lsp_server::Message>) -> Self {
        GlobalState {
            sender,
            root_path: None,
            parser: Parser::new(),
            file_results: HashMap::new(),
            items_by_key: HashMap::new(),
        }
    }
}
