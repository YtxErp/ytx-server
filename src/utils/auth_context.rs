use crate::build_url;
use crate::constant::AUTH_READWRITE_ROLE;
use crate::create_pool;

use anyhow::{Context, Result, bail};
use sqlx::PgPool;
use std::env::var;

/// Context holding only auth DB connection
pub struct AuthContext {
    pub auth_pool: PgPool,
    pub base_postgres_url: String,
}

impl AuthContext {
    /// Initialize context: only DB connection
    pub async fn new(auth_readwrite_password: &str) -> Result<Self> {
        let base_postgres_url =
            var("BASE_POSTGRES_URL").unwrap_or_else(|_| "postgres://localhost:5432".to_string());

        let auth_db = read_value_with_default("AUTH_DB", "ytx_auth")?;
        let auth_url = build_url(
            &base_postgres_url,
            AUTH_READWRITE_ROLE,
            auth_readwrite_password,
            &auth_db,
        )?;

        let auth_pool = create_pool(&auth_url).await?;

        sqlx::query("SELECT 1")
            .execute(&auth_pool)
            .await
            .context("Failed to connect to auth DB")?;

        Ok(Self {
            auth_pool,
            base_postgres_url,
        })
    }
}

fn read_value_with_default(key: &str, default: &str) -> Result<String> {
    let val = var(key).unwrap_or(default.to_string());

    if val.is_empty() {
        bail!("Value for '{}' cannot be empty", key);
    }

    if val.len() > 63 {
        bail!("Value for '{}' cannot be longer than 63 characters", key);
    }

    let mut chars = val.chars();
    let first = chars.next().unwrap();

    if !first.is_ascii_lowercase() {
        bail!("Value for '{}' must start with a lowercase letter", key);
    }

    if !val
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        bail!(
            "Value for '{}' can only contain lowercase letters, digits, and underscore",
            key
        );
    }

    Ok(val)
}
