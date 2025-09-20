use crate::vault::VaultManager;
use anyhow::{Result, anyhow};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::Value;
use std::env::var;

pub struct VaultContext {
    addr: String,
    manager: VaultManager,
    client: reqwest::Client,
}

impl VaultContext {
    pub async fn new() -> Result<Self> {
        let role_id = get_env_var("VAULT_ROLE_ID")?;
        let secret_id = get_env_var("VAULT_SECRET_ID")?;
        let addr = var("VAULT_ADDR").unwrap_or_else(|_| "http://127.0.0.1:8200".to_string());

        let token_manager = VaultManager::new(addr.clone(), role_id, secret_id);
        token_manager.get_token().await?;

        Ok(Self {
            addr,
            manager: token_manager,
            client: reqwest::Client::new(),
        })
    }

    pub async fn get_password(&self, secret_path: &str, key: &str) -> Result<String> {
        let token = self.manager.get_token().await?;
        let data = self.read_data(&token, secret_path).await?;

        data.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Vault key '{}' not found or not a string", key))
    }

    async fn read_data(&self, token: &str, secret_path: &str) -> Result<Value> {
        let url = format!("{}/v1/{}", self.addr.trim_end_matches('/'), secret_path);

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))?,
        );

        let resp = self.client.get(&url).headers(headers).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP error {}", resp.status());
        }

        let json: Value = resp.json().await?;
        Ok(json["data"]["data"].clone())
    }
}

fn get_env_var(name: &str) -> Result<String> {
    let val = var(name).map_err(|_| anyhow!("{} not set", name))?;
    if val.trim().is_empty() {
        anyhow::bail!("{} is empty", name);
    }
    Ok(val)
}
