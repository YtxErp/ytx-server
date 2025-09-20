mod constant;
mod dbhub;
mod message;
mod utils;
mod vault;
mod websocket;

use crate::constant::AUTH_READWRITE_ROLE;
use crate::constant::YTX_SECRET_PATH;
use crate::dbhub::*;
use crate::utils::*;
use crate::vault::*;

use anyhow::Result;
use dotenvy::dotenv;
use std::{env::var, sync::Arc};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tracing::info;

use websocket::WebSocket;

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv().ok();

    let rust_log = var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let _guard = init_tracing(&rust_log);

    let vault_context = VaultContext::new().await?;
    let auth_readwrite_password = vault_context
        .get_password(YTX_SECRET_PATH, AUTH_READWRITE_ROLE)
        .await?;

    let auth_context = AuthContext::new(&auth_readwrite_password).await?;

    let db_hub = Arc::new(DbHub::new(vault_context, auth_context));
    let sql_factory = Arc::new(SqlFactory::new());

    {
        let hub = Arc::clone(&db_hub);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(86400));
            loop {
                interval.tick().await;
                hub.cleanup_idle_resource(86400).await;
            }
        });
    }

    let listen_addr = var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:7749".to_string());
    let listener = TcpListener::bind(listen_addr.clone()).await?;

    while let Ok((mut stream, _)) = listener.accept().await {
        let db_hub = db_hub.clone();
        let sql_factory = sql_factory.clone();

        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            let n = match stream.peek(&mut buf).await {
                Ok(n) => n,
                Err(_) => return,
            };

            let request = String::from_utf8_lossy(&buf[..n]);

            if request.starts_with("GET / ") || request.starts_with("GET / HTTP/") {
                if request.to_ascii_lowercase().contains("upgrade: websocket") {
                    info!("Upgrading incoming WebSocket handshake.");

                    WebSocket::new(stream, db_hub, sql_factory).handle().await;
                } else {
                    info!("HTTP request received, responded 200 OK.");

                    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
                    let _ = stream.write_all(response).await;
                    let _ = stream.flush().await;
                }
                return;
            }
        });
    }

    Ok(())
}
