//! A private metadata document. Response bodies and script logs stay in separate files.
use crate::{
    runner::{PreparedRun, RunOutcome},
    state::Storage,
};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::Url;

pub struct Console {
    pub path: PathBuf,
    heading: String,
}

impl Console {
    pub fn new(storage: &Storage, source: &Path, request: &PreparedRun) -> Self {
        Self {
            path: storage.console_path(source),
            heading: format!(
                "# Sendline · {}\n\n{} {}\n\nEnvironment: {}\n\n",
                text(&request.request.name),
                text(&request.request.method),
                text(&request.endpoint),
                text(request.environment.as_deref().unwrap_or("none"))
            ),
        }
    }

    pub fn sending(&self, storage: &Storage) -> Result<()> {
        self.write(
            storage,
            "**Sending…**\n\nUse Cancel Request in the request file to stop.\n",
        )
    }

    pub fn completed(&self, storage: &Storage, outcome: &RunOutcome) -> Result<()> {
        let response = &outcome.response;
        let mut body = format!(
            "**{}** · {} ms · {} · {}\n\n",
            response.status,
            response.duration_ms,
            super::size(response.bytes.len()),
            text(&response.content_type)
        );
        body.push_str(&format!(
            "[Open response](<{}>) · [Open headers](<{}>)\n\n",
            file_uri(&outcome.saved.body)?,
            file_uri(&outcome.saved.headers)?
        ));
        if let Some(script) = &outcome.script {
            let passed = script
                .report
                .tests
                .iter()
                .filter(|test| test.passed)
                .count();
            let failed = script.report.tests.len() - passed;
            body.push_str(&format!(
                "Tests: {passed} passed · {failed} failed. [Open test report](<{}>)\n\n",
                file_uri(&script.path)?
            ));
            if script.report.error.is_some() {
                body.push_str(
                    "The response handler failed. Its details are in the private test report.\n\n",
                );
            }
        }
        if !outcome.successful() {
            body.push_str(
                "The request or its response tests failed. The response files remain available.\n",
            );
        }
        self.write(storage, &body)
    }

    pub fn failed(&self, storage: &Storage, message: &str) -> Result<()> {
        self.write(
            storage,
            &format!("**Request failed**\n\n{}\n", text(message)),
        )
    }

    pub fn cancelled(&self, storage: &Storage) -> Result<()> {
        self.write(storage, "**Cancelled**\n\nThe request was stopped.\n")
    }

    fn write(&self, storage: &Storage, body: &str) -> Result<()> {
        storage.write(&self.path, format!("{}{body}\n---\nGenerated metadata. Response bodies and script logs are stored separately.\n", self.heading).as_bytes())?;
        // Zed can reload this generated document, but ordinary editor saves must not change it.
        let mut permissions = std::fs::metadata(&self.path)?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&self.path, permissions)?;
        Ok(())
    }
}

fn file_uri(path: &Path) -> Result<Url> {
    Url::from_file_path(path)
        .ok()
        .context("Cannot open the private response file")
}

fn text(value: &str) -> String {
    let mut result = String::new();
    for character in value.chars() {
        if character.is_control() {
            result.push(' ');
        } else {
            if "\\`*_[]<>()#|~".contains(character) {
                result.push('\\');
            }
            result.push(character);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn untrusted_metadata_cannot_create_markdown_links_or_lines() {
        assert_eq!(
            super::text("[open](file:///private)\n# next"),
            "\\[open\\]\\(file:///private\\) \\# next"
        );
    }
}
