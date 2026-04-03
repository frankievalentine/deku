use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use deku_core::types::ObjectStoreConfig;
use hmac::{Hmac, Mac};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::{Client, Method, Url};
use sha2::{Digest, Sha256};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub async fn test_config(cfg: &ObjectStoreConfig) -> Result<()> {
    let prefix = cfg.normalized_prefix().unwrap_or_default();
    let key = format!(
        "{prefix}deku-objectstore-test-{}.txt",
        Uuid::new_v4().simple()
    );
    let payload = b"deku object store connectivity test".to_vec();

    put_bytes(cfg, &key, payload.clone()).await?;
    let downloaded = get_bytes(cfg, &key).await?;
    if downloaded != payload {
        return Err(anyhow!("downloaded object did not match uploaded payload"));
    }
    delete_object(cfg, &key).await?;

    Ok(())
}

pub async fn put_bytes(cfg: &ObjectStoreConfig, key: &str, payload: Vec<u8>) -> Result<()> {
    let client = Client::new();
    let response = signed_request(&client, Method::PUT, cfg, key, payload).await?;
    ensure_success(response, "upload object").await
}

pub async fn get_bytes(cfg: &ObjectStoreConfig, key: &str) -> Result<Vec<u8>> {
    let client = Client::new();
    let response = signed_request(&client, Method::GET, cfg, key, Vec::new()).await?;
    read_success_bytes(response, "download object").await
}

pub async fn delete_object(cfg: &ObjectStoreConfig, key: &str) -> Result<()> {
    let client = Client::new();
    let response = signed_request(&client, Method::DELETE, cfg, key, Vec::new()).await?;
    ensure_success(response, "delete object").await
}

pub fn normalized_app_prefix(
    cfg: &ObjectStoreConfig,
    app_name: &str,
    override_prefix: Option<&str>,
) -> String {
    if let Some(prefix) = override_prefix {
        let trimmed = prefix.trim().trim_matches('/');
        if !trimmed.is_empty() {
            return format!("{trimmed}/");
        }
    }

    let base = cfg.normalized_prefix().unwrap_or_default();
    format!("{base}apps/{app_name}/")
}

async fn signed_request(
    client: &Client,
    method: Method,
    cfg: &ObjectStoreConfig,
    key: &str,
    payload: Vec<u8>,
) -> Result<reqwest::Response> {
    let url = object_url(cfg, key)?;
    let payload_hash = hex_sha256(&payload);
    let timestamp = Utc::now();
    let amz_date = timestamp.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = timestamp.format("%Y%m%d").to_string();
    let host = url_host_header(&url)?;
    let canonical_uri = canonical_uri(&url);
    let canonical_query = canonical_query(&url);
    let canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str(),
        canonical_uri,
        canonical_query,
        canonical_headers,
        signed_headers,
        payload_hash
    );
    let credential_scope = format!("{date_stamp}/{}/s3/aws4_request", cfg.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        credential_scope,
        hex_sha256(canonical_request.as_bytes())
    );
    let signing_key = signing_key(&cfg.secret_access_key, &date_stamp, &cfg.region)?;
    let signature = hex::encode(hmac_bytes(&signing_key, &string_to_sign)?);
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        cfg.access_key_id, credential_scope, signed_headers, signature
    );

    client
        .request(method, url)
        .header("x-amz-content-sha256", payload_hash)
        .header("x-amz-date", amz_date)
        .header("Authorization", authorization)
        .body(payload)
        .send()
        .await
        .context("send object store request")
}

fn object_url(cfg: &ObjectStoreConfig, key: &str) -> Result<Url> {
    let mut url = Url::parse(&cfg.endpoint).context("parse object store endpoint")?;
    let encoded_key = encode_object_key(key);
    if cfg.path_style {
        let mut path = url.path().trim_end_matches('/').to_string();
        path.push('/');
        path.push_str(&utf8_percent_encode(&cfg.bucket, PATH_SEGMENT_ENCODE_SET).to_string());
        if !encoded_key.is_empty() {
            path.push('/');
            path.push_str(&encoded_key);
        }
        url.set_path(&path);
    } else {
        let host = url
            .host_str()
            .ok_or_else(|| anyhow!("object store endpoint is missing a host"))?;
        url.set_host(Some(&format!("{}.{}", cfg.bucket, host)))
            .context("set virtual-host object store host")?;
        let path = if encoded_key.is_empty() {
            "/".to_string()
        } else {
            format!("/{encoded_key}")
        };
        url.set_path(&path);
    }
    Ok(url)
}

fn encode_object_key(key: &str) -> String {
    key.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| utf8_percent_encode(segment, PATH_SEGMENT_ENCODE_SET).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn canonical_uri(url: &Url) -> String {
    if url.path().is_empty() {
        "/".to_string()
    } else {
        url.path().to_string()
    }
}

fn canonical_query(url: &Url) -> String {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn url_host_header(url: &Url) -> Result<String> {
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("object store endpoint is missing a host"))?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

fn signing_key(secret: &str, date_stamp: &str, region: &str) -> Result<Vec<u8>> {
    let k_date = hmac_bytes(format!("AWS4{secret}").as_bytes(), date_stamp)?;
    let k_region = hmac_bytes(&k_date, region)?;
    let k_service = hmac_bytes(&k_region, "s3")?;
    hmac_bytes(&k_service, "aws4_request")
}

fn hmac_bytes(key: &[u8], message: &str) -> Result<Vec<u8>> {
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|_| anyhow!("invalid HMAC signing key"))?;
    mac.update(message.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

async fn ensure_success(response: reqwest::Response, action: &str) -> Result<()> {
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(anyhow!("{action} failed with HTTP {status}: {body}"))
}

async fn read_success_bytes(response: reqwest::Response, action: &str) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("{action} failed with HTTP {status}: {body}"));
    }
    Ok(response.bytes().await?.to_vec())
}

#[cfg(test)]
mod tests {
    use super::{canonical_uri, encode_object_key, object_url};
    use deku_core::types::ObjectStoreConfig;

    fn sample_config() -> ObjectStoreConfig {
        ObjectStoreConfig {
            provider: "r2".to_string(),
            bucket: "deku".to_string(),
            region: "auto".to_string(),
            endpoint: "https://example.com".to_string(),
            access_key_id: "key".to_string(),
            secret_access_key: "secret".to_string(),
            path_style: true,
            prefix: Some("artifacts".to_string()),
        }
    }

    #[test]
    fn encodes_object_keys_by_segment() {
        assert_eq!(
            encode_object_key("folder/hello world.txt"),
            "folder/hello%20world.txt"
        );
    }

    #[test]
    fn builds_path_style_url() {
        let cfg = sample_config();
        let url = object_url(&cfg, "folder/test.txt").expect("url should build");
        assert_eq!(url.as_str(), "https://example.com/deku/folder/test.txt");
        assert_eq!(canonical_uri(&url), "/deku/folder/test.txt");
    }

    #[test]
    fn builds_virtual_host_url() {
        let mut cfg = sample_config();
        cfg.path_style = false;
        let url = object_url(&cfg, "folder/test.txt").expect("url should build");
        assert_eq!(url.as_str(), "https://deku.example.com/folder/test.txt");
    }
}
