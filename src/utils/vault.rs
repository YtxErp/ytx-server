use anyhow::Result;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::Value;

pub async fn read_vault_data(vault_addr: &str, token: &str, secret_path: &str) -> Result<Value> {
    let url = format!("{}/v1/{}", vault_addr.trim_end_matches('/'), secret_path);

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token))?,
    );

    let client = reqwest::Client::new();
    let resp = client.get(&url).headers(headers).send().await?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP error {}", resp.status());
    }

    let json: Value = resp.json().await?;
    Ok(json["data"]["data"].clone())
}

pub fn get_vault_password(data: &serde_json::Value, key: &str) -> Result<String> {
    data.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Vault key '{}' not found or not a string", key))
}
