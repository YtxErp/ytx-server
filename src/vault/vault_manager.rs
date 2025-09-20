use anyhow::{Context, Result, anyhow};
use reqwest::header::AUTHORIZATION;
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub struct VaultManager {
    addr: String,
    role_id: String,
    secret_id: String,
    client: reqwest::Client,
    token: Mutex<Option<String>>,
    expires_at: Mutex<Option<Instant>>,
}

impl VaultManager {
    pub fn new(vault_addr: String, role_id: String, secret_id: String) -> Self {
        Self {
            addr: vault_addr,
            role_id,
            secret_id,
            client: reqwest::Client::new(),
            token: Mutex::new(None),
            expires_at: Mutex::new(None),
        }
    }

    async fn login(&self) -> Result<String> {
        let url = format!("{}/v1/auth/approle/login", self.addr.trim_end_matches('/'));

        let body = serde_json::json!({
            "role_id": self.role_id,
            "secret_id": self.secret_id,
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send login request to Vault")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let error_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("AppRole login failed: HTTP {} - {}", status, error_text);
        }

        let json: Value = resp
            .json()
            .await
            .context("Failed to parse Vault response as JSON")?;

        let auth = json
            .get("auth")
            .ok_or_else(|| anyhow!("Missing auth in Vault response"))?;

        let token = auth
            .get("client_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing client_token in Vault response"))?
            .to_string();

        let ttl_secs = auth
            .get("lease_duration")
            .and_then(|v| v.as_u64())
            .unwrap_or(3600);

        let expires_at = Instant::now() + Duration::from_secs(ttl_secs);

        {
            let mut token_guard = self.token.lock().await;
            let mut expires_guard = self.expires_at.lock().await;

            *token_guard = Some(token.clone());
            *expires_guard = Some(expires_at);
        }

        Ok(token)
    }

    async fn renew(&self) -> Result<()> {
        let token = {
            let token_guard = self.token.lock().await;
            token_guard
                .as_ref()
                .ok_or_else(|| anyhow!("No token to renew"))?
                .clone()
        };

        let url = format!(
            "{}/v1/auth/token/renew-self",
            self.addr.trim_end_matches('/')
        );

        let resp = self
            .client
            .post(&url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .send()
            .await
            .context("Failed to send renew request to Vault")?;

        if !resp.status().is_success() {
            self.login().await?;
            return Ok(());
        }

        let json: Value = resp
            .json()
            .await
            .context("Failed to parse token renewal response")?;

        let ttl_secs = json
            .get("auth")
            .and_then(|auth| auth.get("lease_duration"))
            .and_then(|v| v.as_u64())
            .unwrap_or(3600);

        let expires_at = Instant::now() + Duration::from_secs(ttl_secs);

        {
            let mut expires_guard = self.expires_at.lock().await;
            *expires_guard = Some(expires_at);
        }

        Ok(())
    }

    pub async fn get_token(&self) -> Result<String> {
        let current_token = {
            let token_guard = self.token.lock().await;
            token_guard.clone()
        };

        let now = Instant::now();
        let mut need_login = false;

        if let Some(expires_at) = *self.expires_at.lock().await {
            let remaining = expires_at.saturating_duration_since(now);
            if remaining <= Duration::from_secs(300) {
                if let Err(_) = self.renew().await {
                    need_login = true;
                }
            }
        } else {
            need_login = true;
        }

        if need_login || current_token.is_none() {
            self.login()
                .await
                .context("Failed to login and obtain new token")?;
        }

        let final_token = {
            let token_guard = self.token.lock().await;
            token_guard
                .as_ref()
                .ok_or_else(|| anyhow!("Token should be available after login/renewal"))?
                .clone()
        };

        Ok(final_token)
    }
}
