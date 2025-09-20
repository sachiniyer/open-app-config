use anyhow::Result;
use client::cached::CachedConfigClient;
use client::ConfigClient;
use serde_json::json;
use shared_types::ConfigKey;
use std::process::{Child, Command};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::time::sleep;

struct TestServer {
    process: Child,
    port: u16,
}

impl TestServer {
    fn start() -> Result<Self> {
        let port = 3456; // Use a non-standard port for testing

        // Clean up any previous test storage
        let _ = std::fs::remove_dir_all("/tmp/open-app-config-test");

        // Build the server first
        Command::new("cargo")
            .args(&["build", "--bin", "server", "-p", "server"])
            .output()?;

        // Start the server with proper environment
        let mut process = Command::new("cargo")
            .args(&["run", "--bin", "server", "-p", "server"])
            .env("HOST", "127.0.0.1")
            .env("PORT", port.to_string())
            .env("STORAGE_PATH", "/tmp/open-app-config-test")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        // Wait for server to start
        thread::sleep(Duration::from_secs(3));

        // Check if process is still running
        match process.try_wait() {
            Ok(Some(status)) => {
                anyhow::bail!("Server failed to start with status: {:?}", status);
            }
            Ok(None) => {
                // Process is still running, good
            }
            Err(e) => {
                anyhow::bail!("Failed to check server status: {}", e);
            }
        }

        Ok(TestServer { process, port })
    }

    fn url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        // Kill the server process
        let _ = self.process.kill();
        let _ = self.process.wait();

        // Clean up test storage
        let _ = std::fs::remove_dir_all("/tmp/open-app-config-test");
    }
}

#[tokio::test]
async fn test_e2e_basic_workflow() -> Result<()> {
    let server = TestServer::start()?;
    let client = ConfigClient::new(server.url())?;

    // Wait for server to be ready
    for _ in 0..10 {
        if client.health_check().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    let key = ConfigKey::new("myapp", "production", "database");

    // Test 1: Create a new configuration
    let content = json!({
        "host": "db.example.com",
        "port": 5432,
        "database": "myapp_prod"
    });

    let schema = json!({
        "type": "object",
        "properties": {
            "host": {"type": "string"},
            "port": {"type": "integer"},
            "database": {"type": "string"}
        },
        "required": ["host", "port", "database"]
    });

    let version = client
        .put_config(&key, content.clone(), Some(schema.clone()), None)
        .await?;

    assert!(!version.is_empty());
    println!("Created config with version: {}", version);

    // Test 2: Get the configuration
    let retrieved = client.get_config(&key).await?;
    assert_eq!(retrieved.content, content);
    assert_eq!(retrieved.schema, schema);

    // Test 3: Update the configuration
    let updated_content = json!({
        "host": "new-db.example.com",
        "port": 5432,
        "database": "myapp_prod",
        "ssl": true
    });

    let version2 = client
        .put_config(&key, updated_content.clone(), None, Some(retrieved.version))
        .await?;

    assert!(!version2.is_empty());
    println!("Updated config to version: {}", version2);

    // Test 4: List versions
    let versions = client.list_versions(&key).await?;
    assert!(versions.len() >= 2);

    // Test 5: Get specific version
    let first_version = client
        .get_config_version(&key, &versions[0].version)
        .await?;
    assert_eq!(first_version.content, content);

    // Test 6: List configs
    let configs = client.list_configs(Some("myapp")).await?;
    assert!(configs.contains(&key));

    // Test 7: Delete the configuration
    client.delete_config(&key).await?;

    // Test 8: Verify deletion
    assert!(client.get_config(&key).await.is_err());

    Ok(())
}

#[tokio::test]
async fn test_e2e_cached_client() -> Result<()> {
    let server = TestServer::start()?;

    // Initialize the cached client (ignore error if already initialized)
    let _ = CachedConfigClient::initialize(&server.url());
    let cached_client = CachedConfigClient::instance()?;

    // Regular client for setup
    let client = ConfigClient::new(server.url())?;

    // Wait for server to be ready
    for _ in 0..10 {
        if cached_client.health_check().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    let key1 = ConfigKey::new("app1", "dev", "settings");
    let key2 = ConfigKey::new("app2", "prod", "config");

    // Setup: Create configurations using regular client
    let content1 = json!({"feature_flag": true, "timeout": 30});
    let schema1 = json!({"type": "object"});
    client
        .put_config(&key1, content1.clone(), Some(schema1.clone()), None)
        .await?;

    let content2 = json!({"api_key": "secret", "endpoint": "https://api.example.com"});
    let schema2 = json!({"type": "object"});
    client
        .put_config(&key2, content2.clone(), Some(schema2.clone()), None)
        .await?;

    // Test 1: First fetch should hit the server
    assert_eq!(cached_client.cache_size().await, 0);
    assert!(!cached_client.is_cached(&key1).await);

    let config1 = cached_client.get_config(&key1).await?;
    assert_eq!(config1.content, content1);
    assert_eq!(cached_client.cache_size().await, 1);
    assert!(cached_client.is_cached(&key1).await);

    // Test 2: Second fetch should use cache (we can't directly verify this,
    // but we can check cache state)
    let config1_again = cached_client.get_config(&key1).await?;
    assert_eq!(config1_again.content, content1);
    assert_eq!(config1_again.version, config1.version);
    assert_eq!(cached_client.cache_size().await, 1); // Still 1

    // Test 3: Fetch a different config
    let config2 = cached_client.get_config(&key2).await?;
    assert_eq!(config2.content, content2);
    assert_eq!(cached_client.cache_size().await, 2);

    // Test 4: Check cached keys
    let cached_keys = cached_client.cached_keys().await;
    assert_eq!(cached_keys.len(), 2);
    assert!(cached_keys.contains(&key1));
    assert!(cached_keys.contains(&key2));

    // Test 5: Clear cache
    cached_client.clear_cache().await;
    assert_eq!(cached_client.cache_size().await, 0);
    assert!(!cached_client.is_cached(&key1).await);
    assert!(!cached_client.is_cached(&key2).await);

    // Test 6: After clearing, fetch should work again
    let config1_after_clear = cached_client.get_config(&key1).await?;
    assert_eq!(config1_after_clear.content, content1);
    assert_eq!(cached_client.cache_size().await, 1);

    // Cleanup
    client.delete_config(&key1).await?;
    client.delete_config(&key2).await?;

    Ok(())
}

#[tokio::test]
async fn test_e2e_version_caching() -> Result<()> {
    let server = TestServer::start()?;

    let _ = CachedConfigClient::initialize(&server.url());
    let cached_client = CachedConfigClient::instance()?;
    let client = ConfigClient::new(server.url())?;

    // Wait for server to be ready
    for _ in 0..10 {
        if cached_client.health_check().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    let key = ConfigKey::new("versiontest", "staging", "api");

    // Create multiple versions
    let v1_content = json!({"version": 1, "enabled": false});
    let schema = json!({"type": "object"});
    client
        .put_config(&key, v1_content.clone(), Some(schema.clone()), None)
        .await?;

    let v2_content = json!({"version": 2, "enabled": true});
    client
        .put_config(&key, v2_content.clone(), None, None)
        .await?;

    let v3_content = json!({"version": 3, "enabled": true, "new_feature": true});
    client
        .put_config(&key, v3_content.clone(), None, None)
        .await?;

    // Get versions list
    let versions = client.list_versions(&key).await?;
    assert!(versions.len() >= 3);

    // Test 1: Fetch specific versions
    let v1 = cached_client
        .get_config_version(&key, &versions[0].version)
        .await?;
    assert_eq!(v1.content["version"], 1);

    let v2 = cached_client
        .get_config_version(&key, &versions[1].version)
        .await?;
    assert_eq!(v2.content["version"], 2);

    // Test 2: Verify versions are cached separately
    let cache_size_before = cached_client.cache_size().await;

    // Fetch the same versions again (should come from cache)
    let v1_cached = cached_client
        .get_config_version(&key, &versions[0].version)
        .await?;
    assert_eq!(v1_cached.content["version"], 1);
    assert_eq!(v1_cached.version, v1.version);

    let v2_cached = cached_client
        .get_config_version(&key, &versions[1].version)
        .await?;
    assert_eq!(v2_cached.content["version"], 2);
    assert_eq!(v2_cached.version, v2.version);

    // Cache size should not have increased
    assert_eq!(cached_client.cache_size().await, cache_size_before);

    // Test 3: Current version is cached separately from specific versions
    let current = cached_client.get_config(&key).await?;
    assert_eq!(current.content["version"], 3);

    // Cleanup
    client.delete_config(&key).await?;

    Ok(())
}

#[tokio::test]
async fn test_e2e_singleton_persistence() -> Result<()> {
    let server = TestServer::start()?;

    let _ = CachedConfigClient::initialize(&server.url());

    // Get instance multiple times
    let instance1 = CachedConfigClient::instance()?;
    let instance2 = CachedConfigClient::instance()?;

    // Both instances should be the same (Arc pointers should be equal)
    assert!(Arc::ptr_eq(&instance1, &instance2));

    let client = ConfigClient::new(server.url())?;

    // Wait for server
    for _ in 0..10 {
        if instance1.health_check().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    let key = ConfigKey::new("singleton", "test", "config");
    let content = json!({"singleton": true});
    let schema = json!({"type": "object"});

    client
        .put_config(&key, content.clone(), Some(schema), None)
        .await?;

    // Fetch with first instance
    instance1.get_config(&key).await?;
    assert_eq!(instance1.cache_size().await, 1);

    // Second instance should see the same cache
    assert_eq!(instance2.cache_size().await, 1);
    assert!(instance2.is_cached(&key).await);

    // Clear cache from second instance
    instance2.clear_cache().await;

    // First instance should see the cleared cache
    assert_eq!(instance1.cache_size().await, 0);

    // Cleanup
    client.delete_config(&key).await?;

    Ok(())
}

#[tokio::test]
async fn test_e2e_error_handling() -> Result<()> {
    let server = TestServer::start()?;
    let client = ConfigClient::new(server.url())?;

    // Wait for server
    for _ in 0..10 {
        if client.health_check().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    let key = ConfigKey::new("errortest", "dev", "config");

    // Test 1: Get non-existent config
    assert!(client.get_config(&key).await.is_err());

    // Test 2: Delete non-existent config (now returns error after our changes)
    assert!(client.delete_config(&key).await.is_err());

    // Test 3: Create config with invalid content (not an object)
    let invalid_content = json!("just a string");
    let schema = json!({"type": "object"});
    assert!(client
        .put_config(&key, invalid_content, Some(schema), None)
        .await
        .is_err());

    // Test 4: Create config without schema (first version requires schema)
    let content = json!({"valid": true});
    assert!(client
        .put_config(&key, content.clone(), None, None)
        .await
        .is_err());

    // Test 5: Version conflict
    let schema = json!({"type": "object"});
    client
        .put_config(&key, content.clone(), Some(schema), None)
        .await?;

    // Try to update with wrong expected version
    let updated_content = json!({"valid": false});
    assert!(client
        .put_config(
            &key,
            updated_content,
            None,
            Some("wrong-version".to_string())
        )
        .await
        .is_err());

    // Cleanup
    client.delete_config(&key).await?;

    Ok(())
}

#[tokio::test]
async fn test_e2e_concurrent_cached_access() -> Result<()> {
    let server = TestServer::start()?;

    let _ = CachedConfigClient::initialize(&server.url());
    let cached_client = CachedConfigClient::instance()?;
    let client = ConfigClient::new(server.url())?;

    // Wait for server
    for _ in 0..10 {
        if cached_client.health_check().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    // Create test configs
    let mut keys = Vec::new();
    for i in 0..5 {
        let key = ConfigKey::new(format!("app{}", i), "test", "config");
        let content = json!({"index": i, "data": format!("test-{}", i)});
        let schema = json!({"type": "object"});
        client.put_config(&key, content, Some(schema), None).await?;
        keys.push(key);
    }

    // Spawn concurrent tasks to fetch configs
    let mut handles = Vec::new();
    for key in &keys {
        let client_clone = cached_client.clone();
        let key_clone = key.clone();

        let handle = tokio::spawn(async move {
            // Each task fetches the same config multiple times
            for _ in 0..3 {
                let _config = client_clone.get_config(&key_clone).await?;
                sleep(Duration::from_millis(10)).await;
            }
            Result::<(), anyhow::Error>::Ok(())
        });

        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await??;
    }

    // Verify all configs are cached
    assert_eq!(cached_client.cache_size().await, keys.len());
    for key in &keys {
        assert!(cached_client.is_cached(key).await);
    }

    // Cleanup
    for key in keys {
        client.delete_config(&key).await?;
    }

    Ok(())
}
