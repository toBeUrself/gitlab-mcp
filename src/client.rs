use std::{env, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::{Client, Method, Response, header};
use serde_json::Value;

const DEFAULT_MAX_RESPONSE_BYTES: usize = 1_048_576;

#[derive(Clone)]
pub struct GitLabClient {
    base_api_url: String,
    client: Client,
    max_response_bytes: usize,
}

impl GitLabClient {
    pub fn from_env() -> Result<Self> {
        let gitlab_url = required_env("GITLAB_URL")?;
        let token = required_env("GITLAB_TOKEN")?;
        let token_type = env::var("GITLAB_TOKEN_TYPE").unwrap_or_else(|_| "private".into());
        let insecure = env_flag("GITLAB_INSECURE");
        let max_response_bytes = env::var("GITLAB_MAX_RESPONSE_BYTES")
            .ok()
            .map(|value| value.parse::<usize>())
            .transpose()
            .context("GITLAB_MAX_RESPONSE_BYTES must be a positive integer")?
            .unwrap_or(DEFAULT_MAX_RESPONSE_BYTES);
        if max_response_bytes == 0 {
            bail!("GITLAB_MAX_RESPONSE_BYTES must be greater than zero");
        }

        let mut headers = header::HeaderMap::new();
        let mut token = header::HeaderValue::from_str(&token)
            .context("GITLAB_TOKEN contains invalid header characters")?;
        token.set_sensitive(true);
        match token_type.to_ascii_lowercase().as_str() {
            "private" | "pat" => {
                headers.insert("PRIVATE-TOKEN", token);
            }
            "oauth" | "bearer" => {
                let mut value = header::HeaderValue::from_str(&format!(
                    "Bearer {}",
                    token
                        .to_str()
                        .context("GITLAB_TOKEN must be visible ASCII")?
                ))?;
                value.set_sensitive(true);
                headers.insert(header::AUTHORIZATION, value);
            }
            "job" => {
                headers.insert("JOB-TOKEN", token);
            }
            other => bail!("unsupported GITLAB_TOKEN_TYPE: {other}; use private, oauth, or job"),
        }

        let client = Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(insecure)
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("gitlab-mcp/", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            base_api_url: api_url(&gitlab_url),
            client,
            max_response_bytes,
        })
    }

    #[cfg(test)]
    pub fn for_test(base_url: &str, max_response_bytes: usize) -> Self {
        Self {
            base_api_url: api_url(base_url),
            client: Client::new(),
            max_response_bytes,
        }
    }

    pub async fn get(&self, path: &str, query: &[(String, String)]) -> Result<Value> {
        self.send(Method::GET, path, query, None).await
    }

    pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
        self.send(Method::POST, path, &[], Some(body)).await
    }

    pub async fn put(&self, path: &str, body: Value) -> Result<Value> {
        self.send(Method::PUT, path, &[], Some(body)).await
    }

    pub fn max_response_bytes(&self) -> usize {
        self.max_response_bytes
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
    ) -> Result<Value> {
        let url = format!("{}{path}", self.base_api_url);
        let mut request = self.client.request(method, &url).query(query);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("failed to connect to GitLab at {}", self.base_api_url))?;
        self.decode(response).await
    }

    async fn decode(&self, response: Response) -> Result<Value> {
        let status = response.status();
        if let Some(length) = response.content_length()
            && length > self.max_response_bytes as u64
        {
            bail!(
                "GitLab response is {length} bytes, exceeding the configured {} byte limit",
                self.max_response_bytes
            );
        }
        let bytes = response.bytes().await?;
        if bytes.len() > self.max_response_bytes {
            bail!(
                "GitLab response exceeds the configured {} byte limit",
                self.max_response_bytes
            );
        }
        let value: Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        if !status.is_success() {
            let detail = serde_json::to_string(&value).unwrap_or_else(|_| "unknown error".into());
            return Err(anyhow!("GitLab API returned {status}: {detail}"));
        }
        Ok(value)
    }
}

pub fn encode_segment(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

fn api_url(value: &str) -> String {
    let value = value.trim_end_matches('/');
    if value.ends_with("/api/v4") {
        value.to_owned()
    } else {
        format!("{value}/api/v4")
    }
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("missing required environment variable {name}"))
}

pub fn env_flag(name: &str) -> bool {
    env::var(name)
        .is_ok_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_api_url() {
        assert_eq!(
            api_url("https://gitlab.example.com/"),
            "https://gitlab.example.com/api/v4"
        );
        assert_eq!(
            api_url("https://gitlab.example.com/api/v4"),
            "https://gitlab.example.com/api/v4"
        );
    }

    #[test]
    fn encodes_project_and_file_paths_as_one_segment() {
        assert_eq!(encode_segment("team/app"), "team%2Fapp");
        assert_eq!(encode_segment("src/main.rs"), "src%2Fmain%2Ers");
    }
}
