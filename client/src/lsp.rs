//! Zed document state and environment controls. Request execution belongs to the native task.
use crate::{
    environment,
    globals::{self, Globals},
    parser, runner, script, storage,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{RwLock, watch};
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

const TOGGLE_ENV: &str = "http.toggleEnvironments";
const SET_ENV: &str = "http.selectEnvironment";

#[derive(Clone)]
struct Backend {
    client: Client,
    state: Arc<EditorState>,
}

struct EditorState {
    documents: RwLock<HashMap<Url, String>>,
    expanded: RwLock<HashSet<Url>>,
    storage: storage::Storage,
    shutdown: watch::Sender<bool>,
}

impl Backend {
    async fn file(&self, uri: &Url) -> anyhow::Result<parser::HttpFile> {
        let documents = self.state.documents.read().await;
        Ok(parser::parse_http_file(documents.get(uri).ok_or_else(
            || anyhow::anyhow!("Open the .http file first"),
        )?))
    }

    async fn commands(&self, uri: &Url) -> anyhow::Result<Vec<Command>> {
        // Environment controls must survive syntax errors while editing requests.
        let hint = {
            let documents = self.state.documents.read().await;
            parser::environment_hint(
                documents
                    .get(uri)
                    .ok_or_else(|| anyhow::anyhow!("Open the .http file first"))?,
            )
        };
        let path = uri
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("Save the .http file first"))?;
        let environments = environment::Environments::load(&path)?;
        let names = environments.names();
        if names.is_empty() {
            return Ok(vec![]);
        }
        let selected = runner::selected_environment(
            &path,
            hint.as_deref(),
            &environments,
            &self.state.storage,
        )?;
        let expanded = self.state.expanded.read().await.contains(uri);
        let mut commands = vec![Command {
            title: format!(
                "Sendline  ·  Environment: {} {}",
                selected.as_deref().unwrap_or("Select"),
                if expanded { "▴" } else { "▾" }
            ),
            command: TOGGLE_ENV.into(),
            arguments: Some(vec![serde_json::json!(uri)]),
        }];
        if expanded {
            commands.extend(names.into_iter().map(|name| Command {
                title: if selected.as_deref() == Some(&name) {
                    format!("✓ {name}")
                } else {
                    name.clone()
                },
                command: SET_ENV.into(),
                arguments: Some(vec![serde_json::json!(uri), serde_json::json!(name)]),
            }));
        }
        Ok(commands)
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
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
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![TOGGLE_ENV.into(), SET_ENV.into()],
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "Sendline".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }
    async fn initialized(&self, _: InitializedParams) {
        let backend = self.clone();
        let mut shutdown = self.state.shutdown.subscribe();
        tokio::spawn(async move {
            let mut revision = backend.state.storage.globals_revision();
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = shutdown.changed() => break,
                    _ = interval.tick() => {
                        let current = backend.state.storage.globals_revision();
                        if current != revision {
                            revision = current;
                            // The task runs in another process. Refresh dependent requests
                            // after its atomic globals write, without touching editor text.
                            let documents: Vec<_> = backend.state.documents.read().await.keys().cloned().collect();
                            for uri in documents {
                                backend.diagnostics(uri).await;
                            }
                        }
                    }
                }
            }
        });
    }
    async fn shutdown(&self) -> Result<()> {
        self.state.shutdown.send_replace(true);
        Ok(())
    }
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.state
            .documents
            .write()
            .await
            .insert(params.text_document.uri.clone(), params.text_document.text);
        self.diagnostics(params.text_document.uri).await;
        let _ = self.client.code_lens_refresh().await;
    }
    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.pop() {
            self.state
                .documents
                .write()
                .await
                .insert(params.text_document.uri.clone(), change.text);
            self.diagnostics(params.text_document.uri).await;
            let _ = self.client.code_lens_refresh().await;
        }
    }
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.diagnostics(params.text_document.uri).await;
        let _ = self.client.code_lens_refresh().await;
    }
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.state
            .documents
            .write()
            .await
            .remove(&params.text_document.uri);
        self.state
            .expanded
            .write()
            .await
            .remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }
    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;
        let Ok(commands) = self.commands(&uri).await else {
            return Ok(Some(vec![]));
        };
        // Anchor inside the first nonempty line so the header remains separate
        // from every request's native gutter button, even at the start of a file.
        let documents = self.state.documents.read().await;
        let Some(text) = documents.get(&uri) else {
            return Ok(Some(vec![]));
        };
        let (line, content) = text
            .lines()
            .enumerate()
            .find(|(_, line)| !line.is_empty())
            .unwrap_or((0, ""));
        let column = u32::from(!content.is_empty());
        let position = Position::new(line as u32, column);
        Ok(Some(
            commands
                .into_iter()
                .map(|command| CodeLens {
                    range: Range::new(position, position),
                    command: Some(command),
                    data: None,
                })
                .collect(),
        ))
    }
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let Ok(commands) = self.commands(&params.text_document.uri).await else {
            return Ok(Some(vec![]));
        };
        Ok(Some(
            commands
                .into_iter()
                .map(CodeActionOrCommand::Command)
                .collect(),
        ))
    }
    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        let uri = params
            .arguments
            .first()
            .and_then(|value| value.as_str())
            .and_then(|value| Url::parse(value).ok())
            .ok_or_else(|| Error::invalid_params("Missing document URI"))?;
        match params.command.as_str() {
            TOGGLE_ENV => {
                let mut expanded = self.state.expanded.write().await;
                if !expanded.remove(&uri) {
                    expanded.insert(uri.clone());
                }
            }
            SET_ENV => {
                let name = params
                    .arguments
                    .get(1)
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| Error::invalid_params("Missing environment"))?;
                let path = uri
                    .to_file_path()
                    .map_err(|_| Error::invalid_params("Save the .http file first"))?;
                let environments = environment::Environments::load(&path).map_err(invalid)?;
                if !environments.names().iter().any(|value| value == name) {
                    return Err(Error::invalid_params("Unknown environment"));
                }
                self.state
                    .storage
                    .select(
                        &runner::environment_key(&path, &environments).map_err(invalid)?,
                        name,
                    )
                    .map_err(invalid)?;
                self.state.expanded.write().await.remove(&uri);
            }
            _ => return Err(Error::method_not_found()),
        }
        let documents: Vec<_> = self.state.documents.read().await.keys().cloned().collect();
        for uri in documents {
            self.diagnostics(uri).await;
        }
        let _ = self.client.code_lens_refresh().await;
        Ok(None)
    }
}

fn invalid(error: anyhow::Error) -> Error {
    Error::invalid_params(error.to_string())
}

impl Backend {
    async fn diagnostics(&self, uri: Url) {
        let diagnostics = match self.validate_document(&uri).await {
            Ok(diagnostics) => diagnostics,
            Err(error) => vec![diagnostic(error.to_string(), 0)],
        };
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

impl Backend {
    async fn validate_document(&self, uri: &Url) -> anyhow::Result<Vec<Diagnostic>> {
        let file = self.file(uri).await?;
        let path = uri
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("Save the .http file first"))?;
        let environments = environment::Environments::load(&path)?;
        let selected = runner::selected_environment(
            &path,
            file.environment.as_deref(),
            &environments,
            &self.state.storage,
        )?;
        if selected.is_none() && !environments.names().is_empty() {
            return Ok(vec![diagnostic("Select an environment in the header. The previous selection may have been removed.".into(), 0)]);
        }
        let directory = runner::environment_key(&path, &environments)?;
        let globals = Globals::new(&self.state.storage, &directory, selected.as_deref()).load()?;
        let mut variables = globals::variables(&globals);
        variables.extend(environments.variables(selected.as_deref())?);
        variables.extend(file.variables);
        let client = reqwest::Client::builder().build()?;
        let mut diagnostics: Vec<_> = file
            .requests
            .iter()
            .filter_map(|request| {
                crate::http_client::build_request(&client, request, &variables)
                    .err()
                    .map(|error| diagnostic(error.to_string(), request.line))
            })
            .collect();
        diagnostics.extend(
            file.issues
                .iter()
                .map(|issue| diagnostic(issue.message.clone(), issue.lines.start)),
        );
        for request in &file.requests {
            if let Some(handler) = &request.response_handler
                && let Err(error) = script::validate(handler)
            {
                diagnostics.push(diagnostic(error.to_string(), handler.line));
            }
        }
        Ok(diagnostics)
    }
}

fn diagnostic(message: String, line: u32) -> Diagnostic {
    Diagnostic {
        range: Range::new(Position::new(line, 0), Position::new(line, 0)),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("Sendline".into()),
        message,
        ..Default::default()
    }
}

pub async fn run(storage: storage::Storage) -> anyhow::Result<()> {
    #[cfg(unix)]
    storage.register_runner()?;
    let (service, socket) = LspService::new(|client| Backend {
        client,
        state: Arc::new(EditorState {
            documents: RwLock::new(HashMap::new()),
            expanded: RwLock::new(HashSet::new()),
            storage,
            shutdown: watch::channel(false).0,
        }),
    });
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
    Ok(())
}
