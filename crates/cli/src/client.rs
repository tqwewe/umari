use std::{collections::BTreeMap, path::Path};

use anyhow::{Context, Result, anyhow};
use serde::{Serialize, de::DeserializeOwned};
use umari_types::{ErrorCode, ErrorResponse, UploadResponse};
use ureq::{Agent, Body, http::Response};

pub struct ApiClient {
    base_url: String,
    api_key: Option<String>,
    agent: Agent,
}

impl ApiClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        let agent = Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .new_agent();
        ApiClient {
            base_url,
            api_key,
            agent,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn auth_header(&self) -> Option<String> {
        self.api_key.as_ref().map(|k| format!("Bearer {k}"))
    }

    fn check_response(&self, response: Response<Body>) -> Result<Response<Body>> {
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(response);
        }
        let body = response.into_body().read_to_string().unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body) {
            if let Some(msg) = err.error.message {
                return Err(anyhow!("{msg}"));
            }
            let fallback = match err.error.code {
                ErrorCode::InvalidInput => "invalid input",
                ErrorCode::Duplicate => "already exists",
                ErrorCode::NotFound => "not found",
                ErrorCode::Database => "database error",
                ErrorCode::Integrity => "integrity error",
                ErrorCode::Internal => "internal server error",
                ErrorCode::Unauthorized => "unauthorized",
            };
            return Err(anyhow!("{fallback} (status {status})"));
        }
        Err(anyhow!("request failed with status {status}"))
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let mut req = self.agent.get(&self.url(path));
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", &auth);
        }
        let response: Response<Body> = req
            .call()
            .context("connection error")
            .and_then(|r| self.check_response(r))?;

        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read response")?;
        serde_json::from_str(&body).context("failed to parse response")
    }

    pub fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let json_body = serde_json::to_string(body).context("failed to serialize request body")?;

        let mut req = self
            .agent
            .post(&self.url(path))
            .header("Content-Type", "application/json");
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", &auth);
        }
        let response: Response<Body> = req
            .send(&json_body)
            .context("connection error")
            .and_then(|r| self.check_response(r))?;

        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read response")?;
        serde_json::from_str(&body).context("failed to parse response")
    }

    pub fn put<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let json_body = serde_json::to_string(body).context("failed to serialize request body")?;

        let mut req = self
            .agent
            .put(&self.url(path))
            .header("Content-Type", "application/json");
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", &auth);
        }
        let response: Response<Body> = req
            .send(&json_body)
            .context("connection error")
            .and_then(|r| self.check_response(r))?;

        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read response")?;
        serde_json::from_str(&body).context("failed to parse response")
    }

    pub fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let mut req = self.agent.delete(&self.url(path));
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", &auth);
        }
        let response: Response<Body> = req
            .call()
            .context("connection error")
            .and_then(|r| self.check_response(r))?;

        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read response")?;
        serde_json::from_str(&body).context("failed to parse response")
    }

    pub fn upload_wasm(
        &self,
        module_type: &str,
        name: &str,
        version: &str,
        env: &BTreeMap<String, String>,
        file_path: &Path,
        activate: bool,
    ) -> Result<Option<(bool, UploadResponse)>> {
        let wasm_bytes = std::fs::read(file_path)
            .with_context(|| format!("failed to read file: {}", file_path.display()))?;

        // Build multipart body
        let boundary = "----UmariCLIBoundary";
        let mut multipart_body = Vec::new();

        // Add env vars
        for (key, value) in env {
            multipart_body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());

            multipart_body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"env[{key}]\"\r\n\r\n").as_bytes(),
            );

            multipart_body.extend_from_slice(value.as_bytes());
            multipart_body.extend_from_slice(b"\r\n");
        }

        // Add wasm field
        multipart_body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        multipart_body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"wasm\"; filename=\"module.wasm\"\r\n",
        );
        multipart_body.extend_from_slice(b"Content-Type: application/wasm\r\n\r\n");
        multipart_body.extend_from_slice(&wasm_bytes);
        multipart_body.extend_from_slice(b"\r\n");

        // End boundary
        multipart_body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        // Make request
        let url = self.url(&format!(
            "/{module_type}/{name}/versions/{version}?activate={activate}"
        ));

        let mut req = self.agent.post(&url).header(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        );
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", &auth);
        }
        let response: Response<Body> = req.send(&multipart_body).context("connection error")?;

        if response.status().as_u16() == 409 {
            return Ok(None);
        }

        let response = self.check_response(response)?;
        let idempotent = response.status().as_u16() == 200;
        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read response")?;
        let upload_response =
            serde_json::from_str(&body).context("failed to parse upload response")?;
        Ok(Some((idempotent, upload_response)))
    }
}
