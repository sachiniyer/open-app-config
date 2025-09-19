use anyhow::Result;
use shared_types::{ConfigData, ConfigKey};
use storage_backend::{ConfigStorage, ObjectStoreBackend, StorageConfig};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create local storage backend
    let storage_path = std::env::var("STORAGE_PATH").unwrap_or_else(|_| "./data".to_string());
    println!("Using storage path: {}", storage_path);

    let config = StorageConfig::local(storage_path);
    let storage = ObjectStoreBackend::from_config(config)?;

    // Create a config key
    let key = ConfigKey::new("my-app", "production", "database-config");

    // Create config data with schema
    let config_data = ConfigData {
        content: serde_json::json!({
            "host": "db.example.com",
            "port": 5432,
            "database": "myapp",
            "pool_size": 20
        }),
        schema: Some(serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "host": {"type": "string"},
                "port": {"type": "integer"},
                "database": {"type": "string"},
                "pool_size": {"type": "integer", "minimum": 1}
            },
            "required": ["host", "port", "database"]
        })),
        version: "v1".to_string(),
    };

    // Store the config
    println!("\nStoring config for {}", key);
    storage.put(&key, &config_data).await?;

    // Update the config (creates v2)
    let updated_config = ConfigData {
        content: serde_json::json!({
            "host": "db-new.example.com",
            "port": 5432,
            "database": "myapp",
            "pool_size": 30,
            "ssl_enabled": true
        }),
        schema: config_data.schema.clone(),
        version: "v2".to_string(),
    };

    println!("Updating config (creating v2)");
    storage.put(&key, &updated_config).await?;

    // Retrieve current version
    println!("\nRetrieving current version:");
    let current = storage.get(&key).await?;
    println!("Current version: {}", current.version);
    println!(
        "Content: {}",
        serde_json::to_string_pretty(&current.content)?
    );

    // List all versions
    println!("\nListing all versions:");
    let versions = storage.list_versions(&key).await?;
    for version_info in &versions {
        println!(
            "  - {} (created at: {})",
            version_info.version,
            version_info.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }

    // Get specific version
    println!("\nRetrieving v1:");
    let v1 = storage.get_version(&key, "v1").await?;
    println!("Content: {}", serde_json::to_string_pretty(&v1.content)?);

    // List all configs
    println!("\nListing all configs:");
    let all_configs = storage.list(None).await?;
    for config_key in &all_configs {
        println!("  - {}", config_key);
    }

    // Check if config exists
    let exists = storage.exists(&key).await?;
    println!("\nConfig exists: {}", exists);

    Ok(())
}
