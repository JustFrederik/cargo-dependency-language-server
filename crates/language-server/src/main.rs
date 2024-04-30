use dashmap::DashMap;
use ropey::Rope;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use toml::TomlData;
use tower_lsp::jsonrpc::Result as TowerResult;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionOptions, CompletionParams, CompletionResponse,
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, ExecuteCommandParams, InitializedParams, MessageType, OneOf,
    Position, ServerCapabilities, ServerInfo, TextDocumentItem, TextDocumentSyncCapability,
    TextDocumentSyncKind, WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
};
use tower_lsp::Client;
use tower_lsp::{
    lsp_types::{InitializeParams, InitializeResult},
    LanguageServer, LspService, Server,
};

#[tokio::main]
async fn main() {
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let path = std::env::current_dir()
        .unwrap()
        .join("crates.io-index-minfied");

    let (service, socket) = LspService::new(|client| Backend {
        client,
        document_map: Default::default(),
        data: TomlData::new(&path),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}

pub struct Backend {
    document_map: DashMap<String, Rope>,
    data: Arc<Mutex<TomlData>>,
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> TowerResult<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "Cargo Dependency Language Server".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    //TODO: convert to incremental
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                    ..Default::default()
                }),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                ..ServerCapabilities::default()
            },
            ..Default::default()
        })
    }
    async fn shutdown(&self) -> TowerResult<()> {
        Ok(())
    }
    async fn execute_command(&self, _: ExecuteCommandParams) -> TowerResult<Option<Value>> {
        Ok(None)
    }
    async fn initialized(&self, _: InitializedParams) {}

    async fn did_change_workspace_folders(&self, _: DidChangeWorkspaceFoldersParams) {}

    async fn did_change_configuration(&self, _: DidChangeConfigurationParams) {}

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {}

    async fn did_save(&self, _: DidSaveTextDocumentParams) {}

    async fn did_close(&self, _: DidCloseTextDocumentParams) {}

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(TextDocumentItem {
            language_id: "".to_string(),
            uri: params.text_document.uri,
            text: params.text_document.text,
            version: params.text_document.version,
        })
        .await;
    }
    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.on_change(TextDocumentItem {
            language_id: "".to_string(),
            uri: params.text_document.uri,
            text: std::mem::take(&mut params.content_changes[0].text),
            version: params.text_document.version,
        })
        .await
    }
    async fn completion(
        &self,
        params: CompletionParams,
    ) -> TowerResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        if !uri.to_string().ends_with("/Cargo.toml") {
            return Ok(Some(CompletionResponse::Array(vec![])));
        }
        let position = params.text_document_position.position;
        let completions = || -> Option<Vec<CompletionItem>> {
            let rope = self.document_map.get(&uri.to_string())?;
            let char = rope.try_line_to_char(position.line as usize).ok()?;
            let offset = char + position.character as usize;
            let slice = rope.slice(..).as_str().unwrap_or_default();
            if let Some(group) = find_group(&slice, offset) {
                if group.ends_with("dependencies") {
                    let (left, right) = get_line(slice, offset);
                    let details = left.contains("=");
                    return match details {
                        false => Some(
                            self.data
                                .lock()
                                .unwrap()
                                .search(&left)
                                .into_iter()
                                .map(|(name, version)| CompletionItem {
                                    label: name.clone(),
                                    detail: None,
                                    insert_text: Some(format!("{name} = \"{version}\"")),
                                    ..Default::default()
                                })
                                .collect(),
                        ),
                        true => Some(
                            self.details(&left, &right)
                                .map(|(a, b)| {
                                    a.into_iter()
                                        .map(|value| match b {
                                            Some(v) => CompletionItem {
                                                label: value.clone(),
                                                detail: None,
                                                insert_text: Some(format!("{value}{v}")),
                                                ..Default::default()
                                            },
                                            None => CompletionItem {
                                                label: value.clone(),
                                                detail: None,
                                                ..Default::default()
                                            },
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                        ),
                    };
                }
            }
            Some(vec![])
        }();
        self.client
            .log_message(MessageType::INFO, format!("{}", uri))
            .await;
        Ok(completions.map(CompletionResponse::Array))
    }
}

pub fn get_char_index_from_position(s: &str, position: Position) -> usize {
    let line_start = s
        .lines()
        .take(position.line as usize)
        .map(|line| line.len() + 1)
        .sum::<usize>();

    let char_index = line_start + position.character as usize;

    if char_index > s.len() {
        s.len()
    } else {
        char_index
    }
}

pub fn get_line(input: &str, offset: usize) -> (String, String) {
    let (left, right) = if input.is_empty() {
        ("", "")
    } else if offset >= input.chars().count() {
        (input, "")
    } else {
        let byte_offset = input
            .char_indices()
            .enumerate()
            .find(|(index, _)| *index == offset)
            .map(|(_, v)| v.0)
            .unwrap_or_default();
        input.split_at(byte_offset)
    };

    let lindex = left.rfind("\n").map(|i| i + 1).unwrap_or(0);
    let rindex = match right.is_empty() {
        true => 0,
        false => right.find("\n").unwrap_or(right.len()),
    };
    let lslice = match left.len() > lindex {
        true => &left[lindex..],
        false => "",
    };
    let rslice = match right.len() >= rindex {
        true => &right[..rindex],
        false => "",
    };

    (lslice.to_string(), rslice.to_string())
}

impl Backend {
    async fn on_change(&self, params: TextDocumentItem) {
        if !params.uri.to_string().ends_with("/Cargo.toml") {
            return;
        }
        let rope = ropey::Rope::from_str(&params.text);
        self.document_map
            .insert(params.uri.to_string(), rope.clone());
    }

    fn details(&self, left: &str, right: &str) -> Option<(Vec<String>, Option<char>)> {
        let (crate_name, details) = left.split_once("=").unwrap();
        let crate_name = crate_name.trim();
        let details = details.trim();
        if details.starts_with("{") {
            //TODO:
            None
        } else if let Some(str) = details.strip_prefix('"') {
            let versions = self
                .data
                .lock()
                .unwrap()
                .get_versions(crate_name)?
                .into_iter()
                .filter(|ver| ver.starts_with(str))
                .collect();
            Some((
                versions,
                match right.starts_with('"') {
                    true => None,
                    false => Some('"'),
                },
            ))
        } else {
            None
        }
    }
}

pub fn find_group(input: &str, offset: usize) -> Option<String> {
    let byte_offset = input
        .char_indices()
        .enumerate()
        .find(|(index, _)| *index == offset)
        .map(|(_, v)| v.0)
        .unwrap_or_default();
    let input = if byte_offset == 0 {
        input
    } else if byte_offset < input.len() {
        &input[..byte_offset]
    } else {
        input
    };
    if let Some(index) = input.rfind("\n[") {
        if let Some(end_index) = input[index..].find("]\n") {
            return Some(input[index + 2..index + end_index].to_string());
        }
    } else if input.starts_with("[") {
        if let Some(end_index) = input.find("]\n") {
            return Some(input[1..end_index].to_string());
        }
    }
    None
}
