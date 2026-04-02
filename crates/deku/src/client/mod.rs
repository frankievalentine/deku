use anyhow::{anyhow, Result};
use reqwest::{Client, StatusCode};
use std::path::PathBuf;

pub struct DekuClient {
    http: Client,
    pub base_url: String,
    token: Option<String>,
}

fn token_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("/root"))
        .join(".deku")
        .join("cli-token")
}

fn load_token() -> Option<String> {
    std::fs::read_to_string(token_path())
        .ok()
        .map(|s| s.trim().to_string())
}

impl DekuClient {
    pub fn new() -> Result<Self> {
        let token = load_token();
        Ok(Self {
            http: Client::new(),
            base_url: "http://localhost:2810".to_string(),
            token,
        })
    }

    fn auth_header(&self) -> Option<String> {
        self.token.as_ref().map(|t| format!("Bearer {t}"))
    }

    pub async fn get(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{path}", self.base_url);
        let mut req = self.http.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let res = req.send().await?;
        self.handle_response(res).await
    }

    pub async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}{path}", self.base_url);
        let mut req = self.http.post(&url).json(&body);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let res = req.send().await?;
        self.handle_response(res).await
    }

    pub async fn patch(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}{path}", self.base_url);
        let mut req = self.http.patch(&url).json(&body);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let res = req.send().await?;
        self.handle_response(res).await
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{path}", self.base_url);
        let mut req = self.http.delete(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let res = req.send().await?;
        if res.status().is_success() || res.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        Err(anyhow!("HTTP {status}: {body}"))
    }

    /// POST a multipart/form-data body with a tar.gz file upload.
    pub async fn post_archive(
        &self,
        path: &str,
        archive_bytes: Vec<u8>,
    ) -> Result<serde_json::Value> {
        use reqwest::multipart::{Form, Part};
        let url = format!("{}{path}", self.base_url);
        let part = Part::bytes(archive_bytes)
            .file_name("archive.tar.gz")
            .mime_str("application/gzip")?;
        let form = Form::new().part("archive", part);
        let mut req = self.http.post(&url).multipart(form);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let res = req.send().await?;
        self.handle_response(res).await
    }

    /// Stream SSE events, calling `on_line` for each `data:` field.
    pub async fn stream_sse(&self, path: &str, mut on_line: impl FnMut(&str)) -> Result<()> {
        use futures::StreamExt;
        let url = format!("{}{path}", self.base_url);
        let mut req = self.http.get(&url).header("Accept", "text/event-stream");
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let res = req.send().await?;
        let mut stream = res.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf.drain(..=pos);
                if let Some(data) = line.strip_prefix("data: ") {
                    on_line(data);
                }
            }
        }
        Ok(())
    }

    async fn handle_response(&self, res: reqwest::Response) -> Result<serde_json::Value> {
        let status = res.status();
        if status.is_success() {
            let text = res.text().await?;
            if text.is_empty() {
                return Ok(serde_json::Value::Null);
            }
            Ok(serde_json::from_str(&text)?)
        } else {
            let body = res.text().await.unwrap_or_default();
            Err(anyhow!("HTTP {status}: {body}"))
        }
    }
}
