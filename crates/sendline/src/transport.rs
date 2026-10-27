use crate::{request::HttpRequest, request::variables::resolve_variables};
use anyhow::{Result, bail};
use base64::prelude::*;
use reqwest::{Client, Method, Request};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

pub fn build_request(
    client: &Client,
    request: &HttpRequest,
    variables: &HashMap<String, String>,
) -> Result<Request> {
    let url = resolve_variables(&request.url, variables)?;
    let url = reqwest::Url::parse(&url).map_err(|_| anyhow::anyhow!("Invalid request URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("Only http and https URLs are supported");
    }
    let mut builder = client.request(Method::from_bytes(request.method.as_bytes())?, url);
    for (name, value) in &request.headers {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| anyhow::anyhow!("Invalid header name"))?;
        let mut value = resolve_variables(value, variables)?;
        if name == reqwest::header::AUTHORIZATION
            && let Some(credentials) = value.strip_prefix("Basic ")
            && let Some((user, password)) = credentials.split_once(' ')
        {
            value = format!(
                "Basic {}",
                BASE64_STANDARD.encode(format!("{}:{}", user.trim(), password.trim()))
            );
        }
        let mut value = reqwest::header::HeaderValue::from_str(&value)
            .map_err(|_| anyhow::anyhow!("Invalid header value"))?;
        value.set_sensitive(true);
        builder = builder.header(name, value);
    }
    if let Some(body) = &request.body {
        builder = builder.body(resolve_variables(body, variables)?);
    }
    builder
        .build()
        .map_err(|_| anyhow::anyhow!("Could not build HTTP request"))
}

pub struct HttpResponse {
    pub endpoint: String,
    pub status: reqwest::StatusCode,
    pub duration_ms: u128,
    pub bytes: Vec<u8>,
    pub headers: String,
    pub content_type: String,
    pub header_values: Vec<(String, String)>,
}

pub fn display_endpoint(url: &reqwest::Url) -> String {
    let mut display = url.clone();
    let _ = display.set_username("");
    let _ = display.set_password(None);
    display.set_fragment(None);
    if display.query().is_some() {
        // Keep parameter names useful for debugging without displaying query secrets.
        let keys: Vec<_> = display
            .query_pairs()
            .map(|(name, _)| name.into_owned())
            .collect();
        display.set_query(None);
        let mut query = display.query_pairs_mut();
        for key in keys {
            query.append_pair(&key, "[redacted]");
        }
    }
    display.to_string()
}

/// A built request owns the exact URL shown in the terminal. Dynamic variables resolve once.
pub struct PreparedRequest {
    client: Client,
    request: Request,
}
impl PreparedRequest {
    pub fn endpoint(&self) -> String {
        display_endpoint(self.request.url())
    }
}

pub fn prepare(
    request: &HttpRequest,
    variables: &HashMap<String, String>,
) -> Result<PreparedRequest> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            // Header sensitivity hides logs; it does not prevent forwarding custom
            // API keys. Keep every redirect on the original scheme, host and port.
            if attempt.previous().len() >= 10 {
                attempt.error("Too many HTTP redirects")
            } else if attempt
                .previous()
                .first()
                .is_some_and(|original| original.origin() != attempt.url().origin())
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()?;
    let native = build_request(&client, request, variables)?;
    Ok(PreparedRequest {
        client,
        request: native,
    })
}

pub async fn execute(prepared: PreparedRequest) -> Result<HttpResponse> {
    let start = Instant::now();
    let mut response = prepared
        .client
        .execute(prepared.request)
        .await
        .map_err(network_error)?;
    let endpoint = display_endpoint(response.url());
    let status = response.status();
    let mut headers = format!("{:?} {}\n", response.version(), status);
    let mut header_values = Vec::new();
    for (name, value) in response.headers() {
        header_values.push((
            name.to_string(),
            value.to_str().unwrap_or("[binary]").to_string(),
        ));
        headers.push_str(&format!(
            "{name}: {}\n",
            value.to_str().unwrap_or("[binary]")
        ));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown content type")
        .to_string();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            bail!("Response exceeds 10 MiB");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(HttpResponse {
        endpoint,
        status,
        duration_ms: start.elapsed().as_millis(),
        bytes,
        headers,
        content_type,
        header_values,
    })
}

fn network_error(error: reqwest::Error) -> anyhow::Error {
    // reqwest errors may contain URLs with query credentials. Never expose them.
    anyhow::anyhow!(if error.is_timeout() && error.is_connect() {
        "Connection timed out. Check the server address and network."
    } else if error.is_timeout() {
        "HTTP request timed out after 30 seconds"
    } else if error.is_connect() {
        "Connection failed. Check the server address, network, and TLS certificate."
    } else if error.is_redirect() {
        "Too many HTTP redirects"
    } else {
        "HTTP transport failed while sending the request or reading the response"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_nested_variables_and_rejects_missing_or_cyclic_values() {
        let mut vars = HashMap::from([
            ("base".into(), "http://localhost:{{port}}".into()),
            ("port".into(), "8080".into()),
        ]);
        assert_eq!(
            resolve_variables("{{ base }}/me", &vars).unwrap(),
            "http://localhost:8080/me"
        );
        assert!(resolve_variables("{{token}}", &vars).is_err());
        vars.insert("port".into(), "{{base}}".into());
        assert!(resolve_variables("{{base}}", &vars).is_err());
    }

    #[test]
    fn preserves_body_and_encodes_basic_auth() {
        let file = crate::request::parse_http_file(
            "POST https://example.com\nAuthorization: Basic user pass\nContent-Type: application/json\n\n{\"a\":1}",
        );
        let request = build_request(&Client::new(), &file.requests[0], &HashMap::new()).unwrap();
        assert_eq!(request.headers()["authorization"], "Basic dXNlcjpwYXNz");
        assert_eq!(request.body().unwrap().as_bytes().unwrap(), b"{\"a\":1}");
    }
}
