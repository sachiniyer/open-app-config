use anyhow::Result;
use client::ConfigClient;
use serde_json::json;
use shared_types::ConfigKey;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

struct TestServer {
    process: Child,
    port: u16,
    #[allow(dead_code)]
    storage_path: String,
}

impl TestServer {
    async fn start() -> Result<Self> {
        // Find an available port
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.local_addr()?.port()
        };

        // Use a unique storage path for this test instance
        let storage_path = format!("/tmp/open-app-config-test-{port}");
        let _ = std::fs::remove_dir_all(&storage_path);

        // Build the server first
        println!("Building server...");
        let output = Command::new("cargo")
            .args(["build", "--bin", "server", "-p", "server"])
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to build server: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        println!("Starting server on port {port}...");
        let mut process = Command::new("cargo")
            .args(["run", "--bin", "server", "-p", "server"])
            .env("HOST", "127.0.0.1")
            .env("PORT", port.to_string())
            .env("STORAGE_PATH", &storage_path)
            .env("RUST_LOG", "info")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;

        // Give the server time to start and verify it's running
        let server_url = format!("http://localhost:{port}");
        let client = ConfigClient::new(&server_url)?;

        for i in 0..20 {  // Try for up to 10 seconds
            // Check if process has exited with error
            match process.try_wait() {
                Ok(Some(status)) => {
                    anyhow::bail!("Server exited early with status: {status:?}");
                }
                Ok(None) => {
                    // Process is still running, check if it's ready
                    if client.health_check().await.unwrap_or(false) {
                        println!("Server is ready after {} ms", i * 500);
                        return Ok(TestServer { process, port, storage_path });
                    }
                }
                Err(e) => anyhow::bail!("Failed to check server status: {e}"),
            }

            thread::sleep(Duration::from_millis(500));
        }

        // If we get here, server didn't become ready in time
        let _ = process.kill();
        anyhow::bail!("Server failed to become ready after 10 seconds")
    }

    fn url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_dir_all(&self.storage_path);
    }
}

#[tokio::test]
async fn test_e2e_basic_workflow() -> Result<()> {
    let server = TestServer::start().await?;
    let client = ConfigClient::new(server.url())?;

    let key = ConfigKey::new("myapp", "production", "database");
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
        }
    });

    // Create config
    let version = client
        .put_config(&key, content.clone(), Some(schema.clone()), None)
        .await?;
    assert!(!version.is_empty());

    // Get config (should be cached after first get)
    let retrieved = client.get_config(&key).await?;
    assert_eq!(retrieved.content, content);
    assert_eq!(retrieved.schema, schema);

    // Update config
    let updated_content = json!({
        "host": "new-db.example.com",
        "port": 5432,
        "database": "myapp_prod"
    });
    let version2 = client
        .put_config(&key, updated_content.clone(), None, Some(retrieved.version))
        .await?;
    assert!(!version2.is_empty());

    // Refresh to get latest
    let refreshed = client.refresh(&key).await?;
    assert_eq!(refreshed.content, updated_content);

    // List versions
    let version_list = client.list_versions(&key).await?;
    assert!(version_list.len() >= 2);

    // Delete entire environment
    client.delete_environment("myapp", "production").await?;

    // Verify deletion
    assert!(client.get_config(&key).await.is_err());

    Ok(())
}

#[tokio::test]
async fn test_caching_behavior() -> Result<()> {
    let server = TestServer::start().await?;
    let client = ConfigClient::new(server.url())?;

    let key = ConfigKey::new("cachetest", "dev", "config");
    let content = json!({"cached": true});
    let schema = json!({"type": "object"});

    // Create config
    client
        .put_config(&key, content.clone(), Some(schema.clone()), None)
        .await?;

    // First get - caching is internal
    let data1 = client.get_config(&key).await?;

    // Second get should use cache internally
    let data2 = client.get_config(&key).await?;
    assert_eq!(data1.version, data2.version);

    // Get again - cache handled internally
    client.get_config(&key).await?;

    // Refresh should update cache internally
    let refreshed = client.refresh(&key).await?;
    assert_eq!(refreshed.content, content);

    // Delete the correct environment
    client.delete_environment("cachetest", "dev").await?;
    Ok(())
}

#[tokio::test]
async fn test_e2e_error_handling() -> Result<()> {
    let server = TestServer::start().await?;
    let client = ConfigClient::new(server.url())?;

    let key = ConfigKey::new("errortest", "dev", "config");

    // Get non-existent config
    assert!(client.get_config(&key).await.is_err());

    // Create config without schema fails
    let content = json!({"valid": true});
    assert!(
        client
            .put_config(&key, content.clone(), None, None)
            .await
            .is_err()
    );

    // Create config with invalid content (not object)
    let invalid_content = json!("just a string");
    let schema = json!({"type": "object"});
    assert!(
        client
            .put_config(&key, invalid_content, Some(schema.clone()), None)
            .await
            .is_err()
    );

    // Create properly then test version conflict
    client
        .put_config(&key, content.clone(), Some(schema), None)
        .await?;
    assert!(
        client
            .put_config(&key, content, None, Some("wrong-version".to_string()))
            .await
            .is_err()
    );

    // Delete the test environment
    client.delete_environment("errortest", "dev").await?;
    Ok(())
}
