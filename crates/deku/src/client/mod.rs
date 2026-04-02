use anyhow::Result;
use reqwest::Client;

pub struct DekuClient {
    http: Client,
    base_url: String,
}

impl DekuClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: Client::new(),
            base_url: "http://localhost:2810".to_string(),
        })
    }

    pub async fn get(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{path}", self.base_url);
        let res = self.http.get(&url).send().await?;
        Ok(res.json().await?)
    }

    pub async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}{path}", self.base_url);
        let res = self.http.post(&url).json(&body).send().await?;
        Ok(res.json().await?)
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{path}", self.base_url);
        self.http.delete(&url).send().await?;
        Ok(())
    }
}
