mod http;
mod storage;

use anyhow::Result;
use std::{net::SocketAddr, sync::Arc};
use tracing::{Level, info};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive(Level::INFO.into()),
        )
        .init();

    info!("Starting Open App Config server");

    // Initialize storage backend from environment
    let storage_config = storage::StorageConfig::from_env()?;
    info!("Using storage backend: {:?}", storage_config);

    // Create local directory if using local storage
    if let storage::StorageConfig::Local { ref path } = storage_config {
        std::fs::create_dir_all(path)?;
    }

    let storage = storage::ObjectStoreBackend::from_config(storage_config)?;
    let storage: Arc<dyn storage::ConfigStorage> = Arc::new(storage);

    // Bind to address - support both BIND_ADDRESS and HOST/PORT for compatibility
    let addr = if let Ok(bind_addr) = std::env::var("BIND_ADDRESS") {
        bind_addr.parse::<SocketAddr>()?
    } else {
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
        format!("{host}:{port}").parse::<SocketAddr>()?
    };

    info!("Starting HTTP server on {}", addr);

    // Start the HTTP server
    http::start_server(storage, addr).await?;

    Ok(())
}
