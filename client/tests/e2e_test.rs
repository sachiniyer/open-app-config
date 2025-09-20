use anyhow::Result;
use client::ConfigClient;
use serde_json::json;
use shared_types::ConfigKey;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;
use tokio::time::sleep;

struct TestServer {
    process: Child,
    port: u16,
}

impl TestServer {
    fn start() -> Result<Self> {
        let port = 3456;
        let _ = std::fs::remove_dir_all("/tmp/open-app-config-test");

        Command::new("cargo")
            .args(&["build", "--bin", "server", "-p", "server"])
            .output()?;

        let mut process = Command::new("cargo")
            .args(&["run", "--bin", "server", "-p", "server"])
            .env("HOST", "127.0.0.1")
            .env("PORT", port.to_string())
            .env("STORAGE_PATH", "/tmp/open-app-config-test")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        thread::sleep(Duration::from_secs(3));

        match process.try_wait() {
            Ok(Some(status)) => anyhow::bail!("Server failed to start with status: {:?}", status),
            Ok(None) => {}
            Err(e) => anyhow::bail!("Failed to check server status: {}", e),
        }

        Ok(TestServer { process, port })
    }

    fn url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_dir_all("/tmp/open-app-config-test");
    }
}

#[tokio::test]
async fn test_e2e_basic_workflow() -> Result<()> {
    let server = TestServer::start()?;
    let client = ConfigClient::new(server.url())?;

    for _ in 0..10 {
        if client.health_check().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

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

    // Get config
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

    // List versions
    let versions = client.list_versions(&key).await?;
    assert!(versions.len() >= 2);

    // Delete config
    client.delete_config(&key).await?;
    assert!(client.get_config(&key).await.is_err());

    Ok(())
}

#[tokio::test]
async fn test_e2e_error_handling() -> Result<()> {
    let server = TestServer::start()?;
    let client = ConfigClient::new(server.url())?;

    for _ in 0..10 {
        if client.health_check().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    let key = ConfigKey::new("errortest", "dev", "config");

    // Get non-existent config
    assert!(client.get_config(&key).await.is_err());

    // Create config without schema fails
    let content = json!({"valid": true});
    assert!(client
        .put_config(&key, content.clone(), None, None)
        .await
        .is_err());

    // Create config with invalid content (not object)
    let invalid_content = json!("just a string");
    let schema = json!({"type": "object"});
    assert!(client
        .put_config(&key, invalid_content, Some(schema.clone()), None)
        .await
        .is_err());

    // Create properly then test version conflict
    client
        .put_config(&key, content.clone(), Some(schema), None)
        .await?;
    assert!(client
        .put_config(&key, content, None, Some("wrong-version".to_string()))
        .await
        .is_err());

    client.delete_config(&key).await?;
    Ok(())
}
