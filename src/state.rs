use aidl_parser::{ast, ParseFileResult, Parser};
use std::{collections::HashMap, path::PathBuf};

pub struct GlobalState {
    pub indexing_state: IndexingState,
    pub sender: crossbeam::channel::Sender<lsp_server::Message>,
    pub root_path: Option<PathBuf>,
    pub parser: Parser<PathBuf>,
    pub file_results: HashMap<PathBuf, ParseFileResult<PathBuf>>,
    pub items_by_key: HashMap<ast::ItemKey, PathBuf>,
}

impl GlobalState {
    pub fn new(sender: crossbeam::channel::Sender<lsp_server::Message>) -> Self {
        GlobalState {
            indexing_state: IndexingState::Idle,
            sender,
            root_path: None,
            parser: Parser::new(),
            file_results: HashMap::new(),
            items_by_key: HashMap::new(),
        }
    }
}

#[derive(PartialEq)]
pub enum IndexingState {
    Idle,
    Indexing,
    Indexed,
    Error,
}
