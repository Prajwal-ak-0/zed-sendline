//! Parse JetBrains-style request blocks without performing IO.
pub mod environment;
pub mod variables;

use anyhow::{Result, bail};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub line: u32,
    pub end_line: u32,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub response_handler: Option<ResponseHandler>,
}

#[derive(Clone, Debug)]
pub struct ResponseHandler {
    pub line: u32,
    pub source: String,
}

#[derive(Debug, Default)]
pub struct HttpFile {
    pub requests: Vec<HttpRequest>,
    pub variables: HashMap<String, String>,
    pub environment: Option<String>,
    pub issues: Vec<ParseIssue>,
}

#[derive(Debug)]
pub struct ParseIssue {
    pub lines: std::ops::Range<u32>,
    pub message: String,
}

impl HttpFile {
    pub fn request_at(&self, line: u32) -> Result<&HttpRequest> {
        if let Some(issue) = self.issues.iter().find(|issue| issue.lines.contains(&line)) {
            bail!("{}", issue.message);
        }
        self.requests
            .iter()
            .find(|request| request.line <= line && line < request.end_line)
            .or_else(|| {
                self.requests.first().filter(|request| {
                    line < request.line
                        && self
                            .issues
                            .iter()
                            .all(|issue| issue.lines.start >= request.line)
                })
            })
            .ok_or_else(|| anyhow::anyhow!("No HTTP request at line {}", line + 1))
    }
}

/// Read the environment hint even while a request is incomplete or unsupported.
pub fn environment_hint(content: &str) -> Option<String> {
    let mut hint = None;
    let mut preamble = true;
    for line in content.lines().map(str::trim) {
        if line.starts_with("###") {
            preamble = true;
        } else if preamble {
            if let Some(name) = line
                .strip_prefix("# @env ")
                .or_else(|| line.strip_prefix("// @env "))
            {
                hint = Some(name.trim().to_string());
            }
            if !line.is_empty() && !line.starts_with(['#', '@']) && !line.starts_with("//") {
                preamble = false;
            }
        }
    }
    hint
}

pub fn parse_http_file(content: &str) -> HttpFile {
    let mut file = HttpFile {
        environment: environment_hint(content),
        ..Default::default()
    };
    let lines: Vec<_> = content.lines().collect();
    let mut start = 0;
    let mut name = String::new();
    for end in 0..=lines.len() {
        if end != lines.len() && !lines[end].trim_start().starts_with("###") {
            continue;
        }
        if let Err(error) = parse_block(&lines[start..end], start, &name, &mut file) {
            file.issues.push(ParseIssue {
                lines: start as u32..end as u32,
                message: error.to_string(),
            });
        }
        if end < lines.len() {
            name = lines[end]
                .trim_start()
                .trim_start_matches('#')
                .trim()
                .to_string();
        }
        start = end + 1;
    }
    file
}

fn parse_block(lines: &[&str], offset: usize, name: &str, file: &mut HttpFile) -> Result<()> {
    let end_line = (offset + lines.len()) as u32;
    let (lines, response_handler) = response_handler(lines, offset)?;
    let mut iter = lines.iter().enumerate();
    let Some((line, request_line)) = iter.find(|(_, line)| {
        let line = line.trim();
        if let Some(declaration) = line.strip_prefix('@') {
            if let Some((key, value)) = declaration.split_once('=') {
                file.variables
                    .insert(key.trim().to_string(), value.trim().to_string());
            }
            return false;
        }
        !line.is_empty() && !line.starts_with('#') && !line.starts_with("//")
    }) else {
        return Ok(());
    };
    let parts: Vec<_> = request_line.split_whitespace().collect();
    let (method, url) = match parts.as_slice() {
        [url]
            if url.starts_with("http://")
                || url.starts_with("https://")
                || url.starts_with("{{") =>
        {
            ("GET", *url)
        }
        [method, url] => (*method, *url),
        [method, url, version] if version.starts_with("HTTP/") => (*method, *url),
        _ => bail!("Invalid request line at line {}", offset + line + 1),
    };
    if !matches!(
        method,
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "TRACE" | "CONNECT"
    ) {
        bail!("Unsupported HTTP method at line {}", offset + line + 1);
    }
    let mut request = HttpRequest {
        line: (offset + line) as u32,
        end_line,
        name: if name.is_empty() {
            method.to_string()
        } else {
            name.to_string()
        },
        method: method.to_string(),
        url: url.to_string(),
        headers: Vec::new(),
        body: None,
        response_handler,
    };
    for (index, raw) in iter {
        let line = raw.trim();
        if line.is_empty() {
            let body = lines[index + 1..].join("\n");
            if !body.trim().is_empty() {
                if body.trim_start().starts_with("< ") {
                    bail!(
                        "External file bodies are not supported, at line {}",
                        offset + index + 2
                    );
                }
                request.body = Some(body.trim_end_matches('\n').to_string());
            }
            break;
        }
        if line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        // JetBrains allows query parameters on following indented lines.
        if request.headers.is_empty()
            && (line.starts_with(['?', '&'])
                || (request.url.ends_with(['?', '&'])
                    && line
                        .split_once('=')
                        .is_some_and(|(key, _)| !key.contains(':'))))
        {
            request.url.push_str(line);
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            bail!(
                "Invalid header at line {}. Separate the body with a blank line.",
                offset + index + 1
            );
        };
        request
            .headers
            .push((key.trim().to_string(), value.trim().to_string()));
    }
    file.requests.push(request);
    Ok(())
}

/// A response handler is metadata after the HTTP message, never a request body.
fn response_handler<'a>(
    lines: &'a [&'a str],
    offset: usize,
) -> Result<(&'a [&'a str], Option<ResponseHandler>)> {
    let Some(index) = lines
        .iter()
        .position(|line| line.trim_start().starts_with('>'))
    else {
        return Ok((lines, None));
    };
    let text = lines[index..].join("\n");
    let directive = text.trim_start().strip_prefix('>').unwrap().trim_start();
    let Some(script) = directive.strip_prefix("{%") else {
        bail!(
            "External response scripts are not supported, at line {}",
            offset + index + 1
        );
    };
    let Some((source, tail)) = script.split_once("%}") else {
        bail!(
            "Unclosed response script at line {}. Add %}} to finish it.",
            offset + index + 1
        );
    };
    if tail.lines().any(|line| {
        let line = line.trim();
        !line.is_empty()
            && !line.starts_with('#')
            && !line.starts_with("//")
            && !line.starts_with("<> ")
    }) {
        bail!(
            "Unexpected text after response script at line {}",
            offset + index + 1
        );
    }
    Ok((
        &lines[..index],
        Some(ResponseHandler {
            line: (offset + index) as u32,
            source: source.to_string(),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jetbrains_layout_without_extra_blank_lines() {
        let file = parse_http_file(
            "@host = http://localhost\r\n### Get user\r\nGET {{host}}/me HTTP/1.1\r\nAuthorization: Bearer {{token}}\r\n\r\n### Create\r\nPOST {{host}}\r\nContent-Type: application/json\r\n\r\n{\r\n  \"name\": \"नमस्ते\"\r\n}\r\n",
        );
        assert_eq!(file.requests.len(), 2);
        assert_eq!(file.requests[0].line, 2);
        assert_eq!(file.requests[0].name, "Get user");
        assert!(file.requests[0].body.is_none());
        assert_eq!(
            file.requests[1].body.as_deref(),
            Some("{\n  \"name\": \"नमस्ते\"\n}")
        );
        assert_eq!(file.variables["host"], "http://localhost");
    }

    #[test]
    fn rejects_bad_headers_and_incomplete_scripts() {
        assert!(
            !parse_http_file("GET http://localhost\nBad Header")
                .issues
                .is_empty()
        );
        assert!(
            !parse_http_file("GET http://localhost\n\n> {% client.test('x');")
                .issues
                .is_empty()
        );
        assert!(
            !parse_http_file("GET http://localhost\n\n> script.js")
                .issues
                .is_empty()
        );
        assert!(
            !parse_http_file("POST http://localhost\n\n< body.json")
                .issues
                .is_empty()
        );
    }

    #[test]
    fn separates_response_handlers_from_get_and_post_bodies() {
        let file = parse_http_file(
            "### Get\nGET http://localhost\n> {% client.assert(response.status === 200); %}\n### Post\nPOST http://localhost\nContent-Type: application/json\n\n{\"value\":1}\n\n> {%\nclient.global.set('result', response.body);\n%}\n",
        );
        assert!(file.requests[0].body.is_none());
        let handler = file.requests[0].response_handler.as_ref().unwrap();
        assert_eq!(handler.line, 2);
        assert!(handler.source.contains("client.assert"));
        assert_eq!(file.requests[1].body.as_deref(), Some("{\"value\":1}"));
        assert_eq!(file.requests[1].response_handler.as_ref().unwrap().line, 9);
        let file = parse_http_file("GET http://localhost\n\n>\t{% client.assert(true); %}");
        assert!(file.requests[0].body.is_none());
        assert!(file.requests[0].response_handler.is_some());
    }

    #[test]
    fn joins_multiline_query_parameters_before_headers() {
        let file = parse_http_file(
            "@tenant = test\n### List\nGET {{host}}/api/{{tenant}}\n    ?use_cache=false\n    &limit=100\nX-API-KEY: {{token}}\n",
        );
        assert_eq!(
            file.requests[0].url,
            "{{host}}/api/{{tenant}}?use_cache=false&limit=100"
        );
        assert_eq!(
            file.requests[0].headers,
            [("X-API-KEY".into(), "{{token}}".into())]
        );
        assert!(file.requests[0].body.is_none());
        let file = parse_http_file("GET {{host}}/api?\n    use_cache=false\nX-Test: key=value");
        assert_eq!(file.requests[0].url, "{{host}}/api?use_cache=false");
        assert_eq!(file.requests[0].headers.len(), 1);
    }

    #[test]
    fn reads_environment_hint_without_parsing_requests_or_body_comments() {
        let text =
            "# @env stage\n### Broken request\nGET {{host}}\ninvalid header\n\n# @env prod\n";
        assert!(!parse_http_file(text).issues.is_empty());
        assert_eq!(environment_hint(text).as_deref(), Some("stage"));
        assert_eq!(
            environment_hint("// @env local\n###\n# @env stage\nunfinished").as_deref(),
            Some("stage")
        );
    }

    #[test]
    fn ignores_empty_blocks_and_reads_environment() {
        let file = parse_http_file("###\n# @env local\n###\nGET http://localhost");
        assert_eq!(file.requests.len(), 1);
        assert_eq!(file.environment.as_deref(), Some("local"));
    }

    #[test]
    fn a_broken_request_does_not_block_an_unrelated_request() {
        let file = parse_http_file(
            "@host = http://localhost\n### Broken\nGET {{host}}/broken\ninvalid header\n### Valid\nGET {{host}}/ok\n",
        );
        assert_eq!(file.issues.len(), 1);
        assert_eq!(file.requests.len(), 1);
        assert!(file.request_at(2).is_err());
        assert!(file.request_at(0).is_err());
        assert_eq!(file.request_at(5).unwrap().url, "{{host}}/ok");
        assert!(file.request_at(999).is_err());
    }
}
