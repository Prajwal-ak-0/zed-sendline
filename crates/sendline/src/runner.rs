//! Prepare and execute one request. CLI and LSP adapters own their presentation.
use crate::{
    request::{self, HttpRequest, environment::Environments},
    response::{self, SavedResponse, script},
    state::{
        Storage,
        globals::{self, Globals},
    },
    transport::{self, HttpResponse, PreparedRequest},
};
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub struct PreparedRun {
    pub request: HttpRequest,
    pub environment: Option<String>,
    pub endpoint: String,
    directory: PathBuf,
    globals: Map<String, Value>,
    transport: PreparedRequest,
}

pub struct RunOutcome {
    pub response: HttpResponse,
    pub saved: SavedResponse,
    pub script: Option<ScriptOutcome>,
}

pub struct ScriptOutcome {
    pub report: script::Report,
    pub path: PathBuf,
}

impl RunOutcome {
    pub fn successful(&self) -> bool {
        self.response.status.is_success()
            && self
                .script
                .as_ref()
                .is_none_or(|script| script.report.successful())
    }
}

pub fn environment_key(path: &Path, environments: &Environments) -> Result<PathBuf> {
    environments
        .directory
        .clone()
        .or_else(|| path.parent().map(Path::to_path_buf))
        .context("Save the .http file before selecting an environment")
}

pub fn selected_environment(
    path: &Path,
    hint: Option<&str>,
    environments: &Environments,
    storage: &Storage,
) -> Result<Option<String>> {
    let names = environments.names();
    if let Some(saved) = storage.selection(&environment_key(path, environments)?)? {
        return Ok(names.contains(&saved).then_some(saved));
    }
    Ok(hint
        .map(str::to_string)
        .or_else(|| names.iter().find(|name| name.as_str() == "local").cloned())
        .or_else(|| (names.len() == 1).then(|| names[0].clone())))
}

/// Capture the exact editor snapshot and resolve dynamic values once, before sending.
pub fn prepare(
    path: &Path,
    text: &str,
    line: u32,
    override_environment: Option<&str>,
    storage: &Storage,
) -> Result<PreparedRun> {
    let file = request::parse_http_file(text);
    let request = file
        .request_at(line.checked_sub(1).context("Request lines start at one")?)?
        .clone();
    let environments = Environments::load(path)?;
    let selected = override_environment
        .map(str::to_string)
        .or(selected_environment(
            path,
            file.environment.as_deref(),
            &environments,
            storage,
        )?);
    if selected.is_none() && !environments.names().is_empty() {
        bail!("Select an environment in the .http header before running this request");
    }
    if let Some(handler) = &request.response_handler {
        script::validate(handler)?;
    }
    let directory = environment_key(path, &environments)?;
    let saved_globals = Globals::new(storage, &directory, selected.as_deref()).load()?;
    let mut variables: HashMap<_, _> = globals::variables(&saved_globals);
    variables.extend(environments.variables(selected.as_deref())?);
    variables.extend(file.variables);
    let transport = transport::prepare(&request, &variables)?;
    let endpoint = transport.endpoint();
    Ok(PreparedRun {
        request,
        environment: selected,
        endpoint,
        directory,
        globals: saved_globals,
        transport,
    })
}

pub async fn execute(prepared: PreparedRun, storage: &Storage) -> Result<RunOutcome> {
    let response = transport::execute(prepared.transport).await?;
    let saved = response::save(storage, &prepared.request, &response)?;
    let script = if let Some(handler) = &prepared.request.response_handler {
        let mut report =
            script::run(handler, &response, &prepared.globals).unwrap_or_else(|error| {
                script::Report {
                    tests: vec![],
                    logs: vec![],
                    error: Some(error.to_string()),
                    changes: vec![],
                }
            });
        if report.error.is_none()
            && let Err(error) = Globals::new(
                storage,
                &prepared.directory,
                prepared.environment.as_deref(),
            )
            .commit(&report.changes)
        {
            report.error = Some(format!("Cannot save response-script variables: {error}"));
        }
        let path = response::save_script_report(storage, &saved, &report)?;
        Some(ScriptOutcome { report, path })
    } else {
        None
    };
    Ok(RunOutcome {
        response,
        saved,
        script,
    })
}

pub async fn run(
    path: &Path,
    line: u32,
    environment: Option<&str>,
    storage: &Storage,
) -> Result<bool> {
    let text = std::fs::read_to_string(path).context("Cannot read the .http file")?;
    let prepared = prepare(path, &text, line, environment, storage)?;
    response::start(
        prepared.environment.as_deref(),
        &prepared.request,
        &prepared.endpoint,
    )?;
    let endpoint = prepared.endpoint.clone();
    let outcome = tokio::select! {
        result = execute(prepared, storage) => result,
        _ = tokio::signal::ctrl_c() => {
            response::cancelled();
            return Ok(false);
        }
    };
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            response::failed(&error);
            return Ok(false);
        }
    };
    response::completed(&outcome.response, &endpoint, &outcome.saved);
    if let Some(script) = &outcome.script {
        response::print_script_report(&script.path, &script.report);
    }
    Ok(outcome.successful())
}
