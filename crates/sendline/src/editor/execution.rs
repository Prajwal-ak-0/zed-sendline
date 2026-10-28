//! Standard LSP request controls, cancellation, and explicit artifact opening.
use super::{Backend, Document, invalid};
use crate::{request, response::console::Console, runner};
use serde::Deserialize;
use std::path::PathBuf;
use tokio::sync::watch;
use tower_lsp::{
    jsonrpc::{Error, Result},
    lsp_types::*,
};

pub(super) const SEND: &str = "sendline.sendRequest";
pub(super) const CANCEL: &str = "sendline.cancelRequest";
pub(super) const OPEN: &str = "sendline.openResult";

pub(super) struct RunState {
    id: uuid::Uuid,
    version: i32,
    line: u32,
    phase: Phase,
}

enum Phase {
    Running {
        cancel: watch::Sender<bool>,
    },
    Finished {
        label: String,
        console: PathBuf,
        response: Option<ResponseFiles>,
    },
}

struct ResponseFiles {
    body: PathBuf,
    headers: PathBuf,
    report: Option<PathBuf>,
}

#[derive(Deserialize)]
struct SendRequest {
    uri: Url,
    line: u32,
    version: i32,
}

#[derive(Clone, Copy, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum Artifact {
    Console,
    Response,
    Headers,
    Tests,
}

impl Backend {
    pub(super) async fn request_lenses(&self, uri: &Url, document: &Document) -> Vec<CodeLens> {
        let runs = self.state.runs.read().await;
        let current = runs.get(uri);
        let mut lenses = Vec::new();
        if let Some(RunState {
            version,
            line,
            phase: Phase::Running { .. },
            ..
        }) = current
        {
            let line = if *version == document.version {
                *line
            } else {
                0
            };
            lenses.push(lens(
                line,
                Command {
                    title: "Cancel Request".into(),
                    command: CANCEL.into(),
                    arguments: Some(vec![serde_json::json!(uri)]),
                },
            ));
        } else {
            for request in request::parse_http_file(&document.text).requests {
                lenses.push(lens(
                    request.line,
                    send_command(uri, document, request.line),
                ));
            }
        }
        if let Some(run) = current {
            let line = if run.version == document.version {
                run.line
            } else {
                0
            };
            lenses.extend(
                result_commands(uri, run)
                    .into_iter()
                    .map(|command| lens(line, command)),
            );
        }
        lenses
    }

    pub(super) async fn request_actions(&self, uri: &Url, line: u32) -> Vec<CodeActionOrCommand> {
        let documents = self.state.documents.read().await;
        let Some(document) = documents.get(uri) else {
            return vec![];
        };
        let runs = self.state.runs.read().await;
        let mut commands = Vec::new();
        if let Some(run) = runs.get(uri) {
            if matches!(run.phase, Phase::Running { .. }) {
                commands.push(Command {
                    title: "Sendline: cancel request".into(),
                    command: CANCEL.into(),
                    arguments: Some(vec![serde_json::json!(uri)]),
                });
            }
            commands.extend(result_commands(uri, run));
        }
        if !runs
            .get(uri)
            .is_some_and(|run| matches!(run.phase, Phase::Running { .. }))
            && let Ok(request) = request::parse_http_file(&document.text).request_at(line)
        {
            let mut command = send_command(uri, document, request.line);
            command.title = "Sendline: send request".into();
            commands.insert(0, command);
        }
        commands
            .into_iter()
            .map(CodeActionOrCommand::Command)
            .collect()
    }

    pub(super) async fn send_request(&self, value: serde_json::Value) -> Result<()> {
        let args: SendRequest = serde_json::from_value(value)
            .map_err(|_| Error::invalid_params("Invalid request snapshot"))?;
        if !self.state.show_document.get().copied().unwrap_or(false) {
            return Err(Error::invalid_params(
                "This editor does not support LSP response documents",
            ));
        }
        let document = self
            .state
            .documents
            .read()
            .await
            .get(&args.uri)
            .cloned()
            .ok_or_else(|| Error::invalid_params("Open the request file first"))?;
        if document.version != args.version {
            return Err(Error::invalid_params(
                "The request changed. Use its refreshed Send Request control",
            ));
        }
        let path = args
            .uri
            .to_file_path()
            .map_err(|_| Error::invalid_params("Save the .http file first"))?;
        let prepared = runner::prepare(
            &path,
            &document.text,
            args.line
                .checked_add(1)
                .ok_or_else(|| Error::invalid_params("Invalid request line"))?,
            None,
            &self.state.storage,
        )
        .map_err(invalid)?;
        let console = Console::new(&self.state.storage, &path, &prepared);
        let id = uuid::Uuid::new_v4();
        let (cancel, cancellation) = watch::channel(false);
        {
            let documents = self.state.documents.read().await;
            if !documents
                .get(&args.uri)
                .is_some_and(|current| current.version == args.version)
            {
                return Err(Error::invalid_params(
                    "The request changed or closed before sending",
                ));
            }
            let mut runs = self.state.runs.write().await;
            if runs
                .get(&args.uri)
                .is_some_and(|run| matches!(run.phase, Phase::Running { .. }))
            {
                return Err(Error::invalid_params(
                    "A request is running in this file. Cancel it before sending another",
                ));
            }
            console.sending(&self.state.storage).map_err(invalid)?;
            runs.insert(
                args.uri.clone(),
                RunState {
                    id,
                    version: args.version,
                    line: prepared.request.line,
                    phase: Phase::Running { cancel },
                },
            );
        }
        if let Err(error) = self.open_document(&console.path, false).await {
            let mut runs = self.state.runs.write().await;
            if runs.get(&args.uri).is_some_and(|run| run.id == id) {
                runs.remove(&args.uri);
            }
            let _ = console.failed(
                &self.state.storage,
                "The editor could not open the response console. No request was sent.",
            );
            return Err(error);
        }
        let backend = self.clone();
        let shutdown = self.state.shutdown.subscribe();
        tokio::spawn(async move {
            let outcome = tokio::select! {
                biased;
                _ = cancelled(cancellation) => None,
                _ = cancelled(shutdown) => None,
                outcome = runner::execute(prepared, &backend.state.storage) => Some(outcome),
            };
            let mut response = None;
            let (label, saved) = match outcome {
                Some(Ok(outcome)) => {
                    let label = format!(
                        "{} · {} ms",
                        outcome.response.status, outcome.response.duration_ms
                    );
                    let saved = console.completed(&backend.state.storage, &outcome);
                    response = Some(ResponseFiles {
                        body: outcome.saved.body,
                        headers: outcome.saved.headers,
                        report: outcome.script.map(|script| script.path),
                    });
                    (label, saved)
                }
                Some(Err(error)) => (
                    "Request failed".into(),
                    console.failed(&backend.state.storage, &format!("{error:#}")),
                ),
                None => (
                    "Cancelled".into(),
                    console.cancelled(&backend.state.storage),
                ),
            };
            if saved.is_err() {
                backend.client.show_message(MessageType::ERROR, "Sendline could not save its response console. Check the extension work-directory permissions.").await;
            }
            if let Some(run) = backend.state.runs.write().await.get_mut(&args.uri)
                && run.id == id
            {
                run.phase = Phase::Finished {
                    label,
                    console: console.path,
                    response,
                };
            }
            backend.diagnostics(args.uri).await;
            let _ = backend.client.code_lens_refresh().await;
        });
        let _ = self.client.code_lens_refresh().await;
        Ok(())
    }

    pub(super) async fn cancel_request(&self, uri: &Url) {
        if let Some(RunState {
            phase: Phase::Running { cancel },
            ..
        }) = self.state.runs.read().await.get(uri)
        {
            let _ = cancel.send(true);
        }
    }

    pub(super) async fn open_result(&self, uri: &Url, value: serde_json::Value) -> Result<()> {
        let artifact: Artifact = serde_json::from_value(value)
            .map_err(|_| Error::invalid_params("Unknown response artifact"))?;
        let path = {
            let runs = self.state.runs.read().await;
            let Some(RunState {
                phase: Phase::Finished {
                    console, response, ..
                },
                ..
            }) = runs.get(uri)
            else {
                return Err(Error::invalid_params("Run the request first"));
            };
            match artifact {
                Artifact::Console => Some(console),
                Artifact::Response => response.as_ref().map(|response| &response.body),
                Artifact::Headers => response.as_ref().map(|response| &response.headers),
                Artifact::Tests => response
                    .as_ref()
                    .and_then(|response| response.report.as_ref()),
            }
            .cloned()
            .ok_or_else(|| Error::invalid_params("This request has no such response artifact"))?
        };
        self.open_document(&path, true).await
    }

    async fn open_document(&self, path: &std::path::Path, take_focus: bool) -> Result<()> {
        let uri = Url::from_file_path(path).map_err(|_| Error::internal_error())?;
        let result = self
            .client
            .show_document(ShowDocumentParams {
                uri,
                external: Some(false),
                take_focus: Some(take_focus),
                selection: None,
            })
            .await?;
        if result {
            Ok(())
        } else {
            Err(Error::invalid_params(
                "The editor could not open the response document",
            ))
        }
    }
}

fn send_command(uri: &Url, document: &Document, line: u32) -> Command {
    Command {
        title: "▶ Send Request".into(),
        command: SEND.into(),
        arguments: Some(vec![
            serde_json::json!({"uri": uri, "line": line, "version": document.version}),
        ]),
    }
}

fn result_commands(uri: &Url, run: &RunState) -> Vec<Command> {
    let Phase::Finished {
        label, response, ..
    } = &run.phase
    else {
        return vec![];
    };
    let mut commands = vec![(label.as_str(), Artifact::Console)];
    if let Some(response) = response {
        commands.extend([
            ("Response", Artifact::Response),
            ("Headers", Artifact::Headers),
        ]);
        if response.report.is_some() {
            commands.push(("Tests", Artifact::Tests));
        }
    }
    commands
        .into_iter()
        .map(|(title, artifact)| Command {
            title: title.into(),
            command: OPEN.into(),
            arguments: Some(vec![serde_json::json!(uri), serde_json::json!(artifact)]),
        })
        .collect()
}

fn lens(line: u32, command: Command) -> CodeLens {
    let position = Position::new(line, 0);
    CodeLens {
        range: Range::new(position, position),
        command: Some(command),
        data: None,
    }
}

async fn cancelled(mut receiver: watch::Receiver<bool>) {
    if !*receiver.borrow_and_update() {
        let _ = receiver.changed().await;
    }
}
