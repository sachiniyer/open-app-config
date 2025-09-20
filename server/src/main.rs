mod http;
mod storage;

use anyhow::Result;
use std::{net::SocketAddr, sync::Arc};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive(Level::INFO.into()),
        )
        .init();

    info!("Starting Open App Config server");

    // Initialize storage backend
    let storage_path = std::env::var("STORAGE_PATH").unwrap_or_else(|_| "./data".to_string());
    info!("Using storage path: {}", storage_path);

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&storage_path)?;

    let storage_config = storage::StorageConfig::local(storage_path);
    let storage = storage::ObjectStoreBackend::from_config(storage_config)?;
    let storage: Arc<dyn storage::ConfigStorage> = Arc::new(storage);

    // Bind to address
    let addr = std::env::var("BIND_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse::<SocketAddr>()?;

    // Start the HTTP server
    http::start_server(storage, addr).await?;

    Ok(())
}
