use std::time::Duration;

use anyhow::{Context, Result, bail};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::Serialize;
use serde_json::{Value, json};
use ureq::{Agent, http};

const BASE_URL: &str = "https://api.motherduck.com";
const TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = concat!("dkdc-md-cli/", env!("CARGO_PKG_VERSION"));

/// Characters that must be percent-encoded in a URL path segment.
const PATH_SEGMENT: &AsciiSet = &CONTROLS.add(b' ').add(b'#').add(b'%').add(b'/').add(b'?');

fn encode_path(s: &str) -> String {
    utf8_percent_encode(s, PATH_SEGMENT).to_string()
}

pub struct MotherduckClient {
    agent: Agent,
    bearer: String,
}

impl std::fmt::Debug for MotherduckClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotherduckClient")
            .field("bearer", &"[redacted]")
            .finish()
    }
}

#[derive(Serialize)]
struct CreateTokenRequest<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token_type: Option<&'a str>,
}

impl MotherduckClient {
    pub fn new(token: &str) -> Result<Self> {
        anyhow::ensure!(!token.is_empty(), "MotherDuck API token must not be empty");
        let agent = Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .build()
            .into();

        Ok(Self {
            agent,
            bearer: format!("Bearer {token}"),
        })
    }

    fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{BASE_URL}{path}");
        let resp = self
            .agent
            .get(&url)
            .header("Authorization", &self.bearer)
            .header("User-Agent", USER_AGENT)
            .call()
            .context("request failed")?;
        handle_response(resp).with_context(|| format!("GET {path}"))
    }

    fn delete(&self, path: &str) -> Result<Value> {
        let url = format!("{BASE_URL}{path}");
        let resp = self
            .agent
            .delete(&url)
            .header("Authorization", &self.bearer)
            .header("User-Agent", USER_AGENT)
            .call()
            .context("request failed")?;
        handle_response(resp).with_context(|| format!("DELETE {path}"))
    }

    fn post_json(&self, path: &str, body: &impl Serialize) -> Result<Value> {
        let url = format!("{BASE_URL}{path}");
        let bytes = serde_json::to_vec(body).context("failed to serialize request")?;
        let resp = self
            .agent
            .post(&url)
            .header("Authorization", &self.bearer)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/json")
            .send(&bytes)
            .context("request failed")?;
        handle_response(resp).with_context(|| format!("POST {path}"))
    }

    fn put_json(&self, path: &str, body: &impl Serialize) -> Result<Value> {
        let url = format!("{BASE_URL}{path}");
        let bytes = serde_json::to_vec(body).context("failed to serialize request")?;
        let resp = self
            .agent
            .put(&url)
            .header("Authorization", &self.bearer)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/json")
            .send(&bytes)
            .context("request failed")?;
        handle_response(resp).with_context(|| format!("PUT {path}"))
    }

    // -- Users --

    pub fn create_user(&self, username: &str) -> Result<Value> {
        self.post_json("/v1/users", &json!({"username": username}))
    }

    pub fn delete_user(&self, username: &str) -> Result<Value> {
        self.delete(&format!("/v1/users/{}", encode_path(username)))
    }

    // -- Tokens --

    pub fn list_tokens(&self, username: &str) -> Result<Value> {
        self.get(&format!("/v1/users/{}/tokens", encode_path(username)))
    }

    pub fn create_token(
        &self,
        username: &str,
        name: &str,
        ttl: Option<u64>,
        token_type: Option<&str>,
    ) -> Result<Value> {
        self.post_json(
            &format!("/v1/users/{}/tokens", encode_path(username)),
            &CreateTokenRequest {
                name,
                ttl,
                token_type,
            },
        )
    }

    pub fn delete_token(&self, username: &str, token_id: &str) -> Result<Value> {
        self.delete(&format!(
            "/v1/users/{}/tokens/{}",
            encode_path(username),
            encode_path(token_id),
        ))
    }

    // -- Ducklings --

    pub fn get_duckling_config(&self, username: &str) -> Result<Value> {
        self.get(&format!("/v1/users/{}/instances", encode_path(username),))
    }

    pub fn set_duckling_config(
        &self,
        username: &str,
        rw_size: &str,
        rs_size: &str,
        rs_flock_size: u32,
    ) -> Result<Value> {
        self.put_json(
            &format!("/v1/users/{}/instances", encode_path(username)),
            &json!({
                "config": {
                    "read_write": { "instance_size": rw_size },
                    "read_scaling": { "instance_size": rs_size, "flock_size": rs_flock_size }
                }
            }),
        )
    }

    // -- Accounts --

    pub fn list_active_accounts(&self) -> Result<Value> {
        self.get("/v1/active_accounts")
    }
}

fn handle_response(mut resp: http::Response<ureq::Body>) -> Result<Value> {
    let status = resp.status().as_u16();
    let text = resp
        .body_mut()
        .read_to_string()
        .context("failed to read response body")?;
    parse_response(status, text)
}

fn parse_response(status: u16, text: String) -> Result<Value> {
    match serde_json::from_str::<Value>(&text) {
        Ok(body) if (200..300).contains(&status) => Ok(body),
        Ok(body) => {
            let message = body
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or(&text);
            bail!("API error ({status}): {message}");
        }
        Err(_) if (200..300).contains(&status) => Ok(Value::String(text)),
        Err(_) => bail!("API error ({status}): {text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_path_preserves_safe_chars() {
        assert_eq!(encode_path("svc_test-user.1"), "svc_test-user.1");
    }

    #[test]
    fn encode_path_encodes_unsafe_chars() {
        assert_eq!(encode_path("a/b?c#d e"), "a%2Fb%3Fc%23d%20e");
    }

    #[test]
    fn parse_response_success_json() {
        let result = parse_response(200, r#"{"username": "svc_test"}"#.into()).unwrap();
        assert_eq!(result["username"], "svc_test");
    }

    #[test]
    fn parse_response_success_non_json() {
        let result = parse_response(200, "plain text response".into()).unwrap();
        assert_eq!(result.as_str().unwrap(), "plain text response");
    }

    #[test]
    fn parse_response_error_with_message() {
        let err = parse_response(404, r#"{"message": "user not found"}"#.into()).unwrap_err();
        assert!(err.to_string().contains("404"));
        assert!(err.to_string().contains("user not found"));
    }

    #[test]
    fn parse_response_error_non_json() {
        let err = parse_response(500, "Internal Server Error".into()).unwrap_err();
        assert!(err.to_string().contains("500"));
        assert!(err.to_string().contains("Internal Server Error"));
    }

    #[test]
    fn parse_response_error_json_without_message() {
        let err = parse_response(400, r#"{"error": "something broke"}"#.into()).unwrap_err();
        assert!(err.to_string().contains("400"));
    }

    #[test]
    fn create_token_request_omits_none_fields() {
        let req = CreateTokenRequest {
            name: "tok",
            ttl: None,
            token_type: None,
        };
        assert_eq!(serde_json::to_value(&req).unwrap(), json!({"name": "tok"}));
    }

    #[test]
    fn create_token_request_includes_all_fields() {
        let req = CreateTokenRequest {
            name: "tok",
            ttl: Some(3600),
            token_type: Some("read_write"),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["name"], "tok");
        assert_eq!(json["ttl"], 3600);
        assert_eq!(json["token_type"], "read_write");
    }
}
