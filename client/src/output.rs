//! Terminal presentation and private response artifacts. This module never sends a request.
use crate::{http_client::HttpResponse, parser::HttpRequest, storage::Storage};
use anyhow::Result;
use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

pub struct SavedResponse {
    pub body: PathBuf,
    pub headers: PathBuf,
}

pub fn start(environment: Option<&str>, request: &HttpRequest, endpoint: &str) -> Result<()> {
    println!(
        "\nSendline  ·  {}  ·  {}",
        clean(environment.unwrap_or("no environment")),
        clean(&request.name)
    );
    println!("{} {}", request.method, endpoint);
    if io::stdout().is_terminal() {
        print!("Sending…  Ctrl-C cancels");
        io::stdout().flush()?;
    }
    Ok(())
}

fn finish_progress() {
    if io::stdout().is_terminal() {
        print!("\r\x1b[2K");
    }
}

pub fn cancelled() {
    finish_progress();
    println!("Cancelled. No response saved.\n");
}

pub fn failed(error: &anyhow::Error) {
    finish_progress();
    println!("Request failed: {error:#}\n");
}

pub fn save(
    storage: &Storage,
    request: &HttpRequest,
    response: &HttpResponse,
) -> Result<SavedResponse> {
    let label = request
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(40)
        .collect::<String>();
    let stem = format!(
        "{}-{}-{}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        request.method,
        label.trim_matches('-'),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let (body, extension) = match serde_json::from_slice::<serde_json::Value>(&response.bytes) {
        Ok(value) => {
            let mut bytes = serde_json::to_vec_pretty(&value)?;
            bytes.push(b'\n');
            (bytes, "json")
        }
        Err(_) => (
            response.bytes.clone(),
            if std::str::from_utf8(&response.bytes).is_ok() {
                "txt"
            } else {
                "bin"
            },
        ),
    };
    let saved = SavedResponse {
        body: storage.response_path(&stem, extension),
        headers: storage.response_path(&stem, "headers.txt"),
    };
    storage.write(&saved.body, &body)?;
    storage.write(&saved.headers, response.headers.as_bytes())?;
    Ok(saved)
}

pub fn completed(response: &HttpResponse, requested_endpoint: &str, saved: &SavedResponse) {
    finish_progress();
    let status = response.status.to_string();
    let color = if response.status.is_success() {
        "32"
    } else if response.status.is_redirection() {
        "33"
    } else {
        "31"
    };
    let status = if io::stdout().is_terminal() {
        format!("\x1b[{color}m{status}\x1b[0m")
    } else {
        status
    };
    println!(
        "{status}  ·  {} ms  ·  {}  ·  {}",
        response.duration_ms,
        size(response.bytes.len()),
        clean(&response.content_type)
    );
    if response.endpoint != requested_endpoint {
        println!("Final URL  {}", response.endpoint);
    }
    println!("Response  {}", saved.body.display());
    println!("Headers   {}", saved.headers.display());
    println!("Cmd-click a filename to open it.\n");
}

pub fn script_report(
    storage: &Storage,
    saved: &SavedResponse,
    report: &crate::script::Report,
) -> Result<()> {
    let path = saved.body.with_extension("tests.json");
    storage.write(&path, &serde_json::to_vec_pretty(report)?)?;
    let passed = report.tests.iter().filter(|test| test.passed).count();
    let failed = report.tests.len() - passed;
    println!("Tests     {passed} passed · {failed} failed");
    if report.error.is_some() {
        println!("Response handler failed. See the report for details.");
    }
    // Assertion messages and logs can contain tokens or response data. Keep them private.
    println!("Report    {}\n", path.display());
    Ok(())
}

fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
