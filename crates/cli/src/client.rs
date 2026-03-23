use std::path::Path;

use anyhow::{Context, Result, anyhow};
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Serialize, de::DeserializeOwned};
use umari_types::UploadResponse;

pub struct ApiClient {
    base_url: String,
}

impl ApiClient {
    pub fn new(base_url: String) -> Self {
        ApiClient { base_url }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let response = ureq::get(&self.url(path))
            .call()
            .map_err(|err| self.handle_error(err))?;

        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read response")?;
        serde_json::from_str(&body).context("failed to parse response")
    }

    pub fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let json_body = serde_json::to_string(body).context("failed to serialize request body")?;

        let response = ureq::post(&self.url(path))
            .header("Content-Type", "application/json")
            .send(&json_body)
            .map_err(|err| self.handle_error(err))?;

        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read response")?;
        serde_json::from_str(&body).context("failed to parse response")
    }

    pub fn put<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let json_body = serde_json::to_string(body).context("failed to serialize request body")?;

        let response = ureq::put(&self.url(path))
            .header("Content-Type", "application/json")
            .send(&json_body)
            .map_err(|err| self.handle_error(err))?;

        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read response")?;
        serde_json::from_str(&body).context("failed to parse response")
    }

    pub fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let response = ureq::delete(&self.url(path))
            .call()
            .map_err(|err| self.handle_error(err))?;

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
        file_path: &Path,
        activate: bool,
    ) -> Result<UploadResponse> {
        // Read file and show progress
        let file_size = std::fs::metadata(file_path)
            .with_context(|| format!("failed to read file metadata: {}", file_path.display()))?
            .len();

        let pb = ProgressBar::new(file_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} uploading {bytes}/{total_bytes}")
                .unwrap(),
        );

        let wasm_bytes = std::fs::read(file_path)
            .with_context(|| format!("failed to read file: {}", file_path.display()))?;

        pb.set_position(file_size);

        // Build multipart body
        let boundary = "----UmariCLIBoundary";
        let mut body = Vec::new();

        // Add wasm field
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"wasm\"; filename=\"module.wasm\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: application/wasm\r\n\r\n");
        body.extend_from_slice(&wasm_bytes);
        body.extend_from_slice(b"\r\n");

        // End boundary
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        // Make request
        let url = self.url(&format!(
            "/{module_type}/{name}/versions/{version}?activate={activate}"
        ));

        let response = ureq::post(&url)
            .header(
                "Content-Type",
                &format!("multipart/form-data; boundary={boundary}"),
            )
            .send(&body)
            .map_err(|err| self.handle_error(err))?;

        pb.finish_and_clear();

        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read response")?;
        serde_json::from_str(&body).context("failed to parse upload response")
    }

    fn handle_error(&self, err: ureq::Error) -> anyhow::Error {
        match err {
            ureq::Error::StatusCode(code) => anyhow!("request failed with status {code}"),
            _ => anyhow!("connection error: {err}"),
        }
    }
}
