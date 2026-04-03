use crate::local_config::{load_optional, LocalDekuConfig};
use anyhow::{anyhow, Result};
use reqwest::{Client, StatusCode};

pub struct DekuClient {
    http: Client,
    pub base_url: String,
    ssh_port: u16,
    global_domain: Option<String>,
}

impl DekuClient {
    pub fn new() -> Result<Self> {
        let config = load_optional()?.unwrap_or_default();
        let socket_path = config.effective_socket_path();
        let http = reqwest::Client::builder()
            .unix_socket(socket_path)
            .build()?;
        Ok(Self {
            http,
            base_url: base_url(&config),
            ssh_port: config.effective_ssh_port(),
            global_domain: config.global_domain,
        })
    }

    pub fn ssh_port(&self) -> u16 {
        self.ssh_port
    }

    pub fn global_domain(&self) -> Option<&str> {
        self.global_domain.as_deref()
    }

    pub async fn get(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{path}", self.base_url);
        let res = self.http.get(&url).send().await?;
        self.handle_response(res).await
    }

    pub async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}{path}", self.base_url);
        let res = self.http.post(&url).json(&body).send().await?;
        self.handle_response(res).await
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{path}", self.base_url);
        let res = self.http.delete(&url).send().await?;
        if res.status().is_success() || res.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or(body);
        Err(anyhow!("HTTP {status}: {message}"))
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
        let res = self.http.post(&url).multipart(form).send().await?;
        self.handle_response(res).await
    }

    /// Stream SSE events, calling `on_line` for each `data:` field.
    pub async fn stream_sse(&self, path: &str, mut on_line: impl FnMut(&str)) -> Result<()> {
        use futures::StreamExt;
        let url = format!("{}{path}", self.base_url);
        let res = self
            .http
            .get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await?;
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
            let message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("error")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned)
                })
                .unwrap_or(body);
            Err(anyhow!("HTTP {status}: {message}"))
        }
    }
}

fn base_url(config: &LocalDekuConfig) -> String {
    format!("http://localhost:{}", config.effective_api_port())
}
