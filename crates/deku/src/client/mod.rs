use anyhow::{anyhow, Result};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::path::PathBuf;

pub struct DekuClient {
    http: Client,
    pub base_url: String,
    token: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ClientConfig {
    data_dir: Option<PathBuf>,
    api_port: Option<u16>,
}

fn config_dir() -> PathBuf {
    std::env::var_os("DEKU_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs_next::home_dir()
                .unwrap_or_else(|| PathBuf::from("/root"))
                .join(".deku")
        })
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

fn load_config() -> ClientConfig {
    let path = config_path();
    if !path.exists() {
        return ClientConfig::default();
    }

    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| toml::from_str::<ClientConfig>(&contents).ok())
        .unwrap_or_default()
}

fn token_path() -> PathBuf {
    let config = load_config();
    config.data_dir.unwrap_or_else(config_dir).join("cli-token")
}

fn base_url() -> String {
    let config = load_config();
    format!("http://localhost:{}", config.api_port.unwrap_or(2810))
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
            base_url: base_url(),
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
