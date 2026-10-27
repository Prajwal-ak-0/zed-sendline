//! Bounded JavaScript execution over response data. No host IO is exposed to scripts.
use crate::{globals::Mutation, http_client::HttpResponse, parser::ResponseHandler};
use anyhow::{Context as _, Result, bail};
use rquickjs::{Context, Function, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::time::{Duration, Instant};

const MAX_SCRIPT_BYTES: usize = 1024 * 1024;
const MAX_REPORT_BYTES: usize = 10 * 1024 * 1024;
const MEMORY_LIMIT: usize = 128 * 1024 * 1024;
const TIME_LIMIT: Duration = Duration::from_secs(1);

#[derive(Debug, Deserialize, Serialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Report {
    pub tests: Vec<TestResult>,
    pub logs: Vec<String>,
    pub error: Option<String>,
    #[serde(skip_serializing)]
    pub changes: Vec<Mutation>,
}

impl Report {
    pub fn successful(&self) -> bool {
        self.error.is_none() && self.tests.iter().all(|test| test.passed)
    }
}

fn runtime() -> Result<(Runtime, Context)> {
    let runtime = Runtime::new().context("Cannot initialize response-script engine")?;
    runtime.set_memory_limit(MEMORY_LIMIT);
    runtime.set_max_stack_size(1024 * 1024);
    let context = Context::full(&runtime).context("Cannot initialize response-script context")?;
    let deadline = Instant::now() + TIME_LIMIT;
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    Ok((runtime, context))
}

pub fn validate(handler: &ResponseHandler) -> Result<()> {
    if handler.source.len() > MAX_SCRIPT_BYTES {
        bail!("Response script exceeds 1 MiB at line {}", handler.line + 1);
    }
    let (_runtime, context) = runtime()?;
    let valid = context.with(|ctx| -> rquickjs::Result<()> {
        let compile: Function =
            ctx.eval(r#"source => new Function('client', 'response', 'console', '\"use strict\";\n' + source)"#)?;
        let _: Function = compile.call((handler.source.as_str(),))?;
        Ok(())
    });
    valid.map_err(|_| {
        anyhow::anyhow!(
            "Invalid JavaScript in response handler at line {}",
            handler.line + 1
        )
    })
}

pub fn run(
    handler: &ResponseHandler,
    response: &HttpResponse,
    globals: &Map<String, Value>,
) -> Result<Report> {
    let body = serde_json::from_slice::<Value>(&response.bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&response.bytes).into_owned()));
    let mut content_type = response.content_type.split(';');
    let mime = content_type.next().unwrap_or("").trim();
    let charset = content_type.find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        name.eq_ignore_ascii_case("charset")
            .then(|| value.trim_matches('"'))
    });
    let input = serde_json::to_string(&serde_json::json!({
        "status": response.status.as_u16(), "body": body,
        "contentType": {"mimeType": mime, "charset": charset},
        "headers": response.header_values, "globals": globals,
    }))?;
    let (_runtime, context) = runtime()?;
    let result = context.with(|ctx| -> rquickjs::Result<String> {
        let execute: Function = ctx.eval(include_str!("script_runtime.js"))?;
        execute.call((input.as_str(), handler.source.as_str()))
    }).map_err(|_| anyhow::anyhow!("Response handler stopped at line {}. Execution is limited to 1 second and 128 MiB.", handler.line + 1))?;
    if result.len() > MAX_REPORT_BYTES {
        bail!("Response-script report exceeds 10 MiB");
    }
    serde_json::from_str(&result).context("Cannot read response-script results")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response() -> HttpResponse {
        HttpResponse {
            endpoint: "http://localhost".into(),
            status: reqwest::StatusCode::OK,
            duration_ms: 1,
            bytes: br#"{"items":[{"id":1}],"token":"private-test-token"}"#.to_vec(),
            headers: String::new(),
            content_type: "application/json; charset=utf-8".into(),
            header_values: vec![
                ("x-test".into(), "yes".into()),
                ("x-test".into(), "again".into()),
            ],
        }
    }

    fn handler(source: &str) -> ResponseHandler {
        ResponseHandler {
            line: 13,
            source: source.into(),
        }
    }

    #[test]
    fn runs_real_javascript_tests_and_captures_structured_globals() {
        let handler = handler(
            r#"
            client.test("List All Client Catalogs", function () {
                client.assert(response.status === 200, "Response status is not 200");
                client.global.set("clientCatalogs", response.body["items"]);
            });
            client.test("headers", () => {
                client.assert(response.headers.valueOf("X-Test") === "yes");
                client.assert(response.headers.valuesOf("x-test").length === 2);
                client.assert(response.contentType.mimeType === "application/json");
            });
            client.log(response.body.token);
        "#,
        );
        validate(&handler).unwrap();
        let report = run(&handler, &response(), &Map::new()).unwrap();
        assert!(report.successful());
        assert_eq!(report.tests.len(), 2);
        assert_eq!(report.logs, ["private-test-token"]);
        let Mutation::Set { name, value } = &report.changes[0] else {
            panic!("Missing global")
        };
        assert_eq!(name, "clientCatalogs");
        assert_eq!(value, &serde_json::json!([{"id": 1}]));
    }

    #[test]
    fn tests_run_after_handler_and_failures_do_not_skip_other_tests() {
        let report = run(
            &handler(
                r#"
            client.test('later', () => client.assert(ready));
            let ready = true;
            client.test('fails', () => client.assert(false, 'expected failure'));
            client.test('continues', () => client.assert(true));
        "#,
            ),
            &response(),
            &Map::new(),
        )
        .unwrap();
        assert!(!report.successful());
        assert!(report.tests[0].passed);
        assert!(!report.tests[1].passed);
        assert!(
            report.tests[1]
                .error
                .as_ref()
                .unwrap()
                .contains("expected failure")
        );
        assert!(report.tests[2].passed);
    }

    #[test]
    fn validates_without_executing_and_bounds_execution() {
        validate(&handler("while (true) {}")).unwrap();
        assert!(validate(&handler("client.test('broken', function( {")).is_err());
        assert!(validate(&handler("with ({}) {}")).is_err());
        assert!(run(&handler("while (true) {}"), &response(), &Map::new()).is_err());
        let report = run(
            &handler("client.test('async', async () => {});"),
            &response(),
            &Map::new(),
        )
        .unwrap();
        assert!(!report.successful());
    }

    #[test]
    fn exposes_no_host_capabilities_and_supports_global_lifecycle() {
        let globals = serde_json::from_value(serde_json::json!({"token":"prior"})).unwrap();
        let report = run(
            &handler(
                r#"
            client.assert(typeof process === 'undefined' && typeof require === 'undefined');
            client.assert(typeof fetch === 'undefined' && typeof std === 'undefined');
            client.assert(client.global.get('token') === 'prior');
            client.global.clear('token');
            client.assert(client.global.isEmpty());
            client.global.set('__proto__', {safe: true});
            client.assert(client.global.get('__proto__').safe);
            client.global.clearAll();
            client.assert(client.global.isEmpty());
        "#,
            ),
            &response(),
            &globals,
        )
        .unwrap();
        assert!(report.successful());
        assert_eq!(report.changes.len(), 3);
    }
}
