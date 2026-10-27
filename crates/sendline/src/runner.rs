//! Coordinate one saved request from source file to terminal result.
use crate::{
    request::{self, environment::Environments},
    response::{self, script},
    state::{
        Storage,
        globals::{self, Globals},
    },
    transport,
};
use anyhow::{Context, Result, bail};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

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
        .or_else(|| {
            if names.len() == 1 {
                names.first().cloned()
            } else {
                None
            }
        }))
}

pub async fn run(
    path: &Path,
    line: u32,
    override_environment: Option<&str>,
    storage: &Storage,
) -> Result<bool> {
    let text = std::fs::read_to_string(path).context("Cannot read the .http file")?;
    let file = request::parse_http_file(&text);
    let request = file.request_at(line - 1)?;
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
    let globals = Globals::new(storage, &directory, selected.as_deref());
    let saved_globals = globals.load()?;
    let mut variables: HashMap<_, _> = globals::variables(&saved_globals);
    variables.extend(environments.variables(selected.as_deref())?);
    variables.extend(file.variables.clone());
    let prepared = transport::prepare(request, &variables)?;
    let endpoint = prepared.endpoint();
    response::start(selected.as_deref(), request, &endpoint)?;
    let response = tokio::select! {
        result = transport::execute(prepared) => result,
        _ = tokio::signal::ctrl_c() => {
            response::cancelled();
            return Ok(false);
        }
    };
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            response::failed(&error);
            return Ok(false);
        }
    };
    let saved = response::save(storage, request, &response)?;
    response::completed(&response, &endpoint, &saved);
    let mut success = response.status.is_success();
    if let Some(handler) = &request.response_handler {
        let mut report = script::run(handler, &response, &saved_globals).unwrap_or_else(|error| {
            script::Report {
                tests: vec![],
                logs: vec![],
                error: Some(error.to_string()),
                changes: vec![],
            }
        });
        if report.error.is_none()
            && let Err(error) = globals.commit(&report.changes)
        {
            report.error = Some(format!("Cannot save response-script variables: {error}"));
        }
        success &= report.successful();
        response::script_report(storage, &saved, &report)?;
    }
    Ok(success)
}
