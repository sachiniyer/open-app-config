use anyhow::Result;
use server::storage::{ConfigStorage, ObjectStoreBackend, StorageConfig};
use shared_types::{ConfigData, ConfigKey};
use tempfile::TempDir;
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::minio::MinIO;

// ============================================================================
// Local Storage Tests
// ============================================================================

fn create_local_test_backend() -> Result<(ObjectStoreBackend, TempDir)> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig::Local {
        path: temp_dir.path().to_path_buf(),
    };
    let backend = ObjectStoreBackend::from_config(config)?;
    Ok((backend, temp_dir))
}

#[tokio::test]
async fn test_local_put_and_get_config() -> Result<()> {
    let (backend, _dir) = create_local_test_backend()?;

    let key = ConfigKey::new("test-app", "dev", "database");
    let data = ConfigData {
        content: serde_json::json!({"host": "localhost", "port": 5432}),
        schema: serde_json::json!({"type": "object"}),
        version: String::new(),
    };

    // First put should succeed with no expected version
    backend.put(&key, &data, None).await?;

    // Get should return the data
    let retrieved = backend.get(&key).await?;
    assert_eq!(retrieved.content, data.content);
    assert_eq!(retrieved.schema, data.schema);
    assert_eq!(retrieved.version, "v1");
    Ok(())
}

#[tokio::test]
async fn test_local_optimistic_concurrency_control() -> Result<()> {
    let (backend, _dir) = create_local_test_backend()?;

    let key = ConfigKey::new("test-app", "prod", "api");
    let data1 = ConfigData {
        content: serde_json::json!({"version": 1}),
        schema: serde_json::json!({"type": "object"}),
        version: String::new(),
    };

    // Create first version
    backend.put(&key, &data1, None).await?;

    // Update with correct version should succeed
    let retrieved = backend.get(&key).await?;
    let data2 = ConfigData {
        content: serde_json::json!({"version": 2}),
        schema: data1.schema.clone(),
        version: String::new(),
    };
    backend.put(&key, &data2, Some(&retrieved.version)).await?;

    // Update with wrong version should fail
    let result = backend.put(&key, &data2, Some("v1")).await;
    assert!(result.is_err());

    Ok(())
}

#[tokio::test]
async fn test_local_versioning() -> Result<()> {
    let (backend, _dir) = create_local_test_backend()?;

    let key = ConfigKey::new("app", "env", "config");

    // Create multiple versions
    for i in 1..=3 {
        let data = ConfigData {
            content: serde_json::json!({"version": i}),
            schema: serde_json::json!({"type": "object"}),
            version: String::new(),
        };

        if i == 1 {
            backend.put(&key, &data, None).await?;
        } else {
            let current = backend.get(&key).await?;
            backend.put(&key, &data, Some(&current.version)).await?;
        }
    }

    // List versions
    let versions = backend.list_versions(&key).await?;
    assert_eq!(versions.len(), 3);

    // Get specific version
    let v1_data = backend.get_version(&key, "v1").await?;
    assert_eq!(v1_data.content["version"], 1);
    assert_eq!(v1_data.version, "v1");

    let v3_data = backend.get_version(&key, "v3").await?;
    assert_eq!(v3_data.content["version"], 3);
    assert_eq!(v3_data.version, "v3");

    Ok(())
}

#[tokio::test]
async fn test_local_delete_environment() -> Result<()> {
    let (backend, _dir) = create_local_test_backend()?;

    // Create configs in different environments
    let configs = vec![
        ("app1", "dev", "config1"),
        ("app1", "dev", "config2"),
        ("app1", "prod", "config1"),
        ("app2", "dev", "config1"),
    ];

    for (app, env, name) in configs {
        let key = ConfigKey::new(app, env, name);
        let data = ConfigData {
            content: serde_json::json!({"test": true}),
            schema: serde_json::json!({"type": "object"}),
            version: String::new(),
        };
        backend.put(&key, &data, None).await?;
    }

    // Delete app1/dev environment
    let deleted = backend.delete_environment("app1", "dev").await?;
    assert_eq!(deleted, 2);

    // Verify deleted configs don't exist
    assert!(
        backend
            .get(&ConfigKey::new("app1", "dev", "config1"))
            .await
            .is_err()
    );
    assert!(
        backend
            .get(&ConfigKey::new("app1", "dev", "config2"))
            .await
            .is_err()
    );

    // Verify other configs still exist
    assert!(
        backend
            .get(&ConfigKey::new("app1", "prod", "config1"))
            .await
            .is_ok()
    );
    assert!(
        backend
            .get(&ConfigKey::new("app2", "dev", "config1"))
            .await
            .is_ok()
    );

    Ok(())
}

// ============================================================================
// S3 Storage Tests
// ============================================================================

async fn setup_minio_with_bucket() -> Result<(ContainerAsync<MinIO>, String)> {
    // Start MinIO container
    let container = MinIO::default()
        .with_env_var("MINIO_ROOT_USER", "minioadmin")
        .with_env_var("MINIO_ROOT_PASSWORD", "minioadmin")
        .start()
        .await?;

    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(9000).await?;
    let endpoint = format!("http://{}:{}", host, port);

    // Wait for MinIO to be ready
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Create bucket using AWS SDK
    use aws_config::BehaviorVersion;
    use aws_sdk_s3::config::{Credentials, Region};

    let creds = Credentials::new("minioadmin", "minioadmin", None, None, "test");
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .endpoint_url(&endpoint)
        .credentials_provider(creds)
        .force_path_style(true)
        .build();

    let s3_client = aws_sdk_s3::Client::from_conf(config);

    // Create the bucket
    s3_client
        .create_bucket()
        .bucket("test-bucket")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create bucket: {}", e))?;

    Ok((container, endpoint))
}

#[tokio::test]
async fn test_s3_put_and_get_config() -> Result<()> {
    let (_container, endpoint) = setup_minio_with_bucket().await?;

    let config = StorageConfig::s3(
        "test-bucket",
        Some("us-east-1".to_string()),
        Some(endpoint),
        Some("minioadmin".to_string()),
        Some("minioadmin".to_string()),
        true,
    );

    let backend = ObjectStoreBackend::from_config(config)?;

    let key = ConfigKey::new("test-app", "test-env", "test-config");
    let data = ConfigData {
        content: serde_json::json!({
            "key": "value",
            "nested": {
                "field": "test"
            }
        }),
        schema: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        }),
        version: String::new(),
    };

    // Create new config
    backend.put(&key, &data, None).await?;

    // Read it back
    let retrieved = backend.get(&key).await?;
    assert_eq!(retrieved.content, data.content);
    assert_eq!(retrieved.schema, data.schema);
    assert_eq!(retrieved.version, "v1");

    Ok(())
}

#[tokio::test]
async fn test_s3_versioning() -> Result<()> {
    let (_container, endpoint) = setup_minio_with_bucket().await?;

    let config = StorageConfig::s3(
        "test-bucket",
        Some("us-east-1".to_string()),
        Some(endpoint),
        Some("minioadmin".to_string()),
        Some("minioadmin".to_string()),
        true,
    );

    let backend = ObjectStoreBackend::from_config(config)?;
    let key = ConfigKey::new("version-test", "prod", "api-config");

    // Create multiple versions
    for i in 1..=5 {
        let data = ConfigData {
            content: serde_json::json!({
                "version": i,
                "data": format!("version-{}", i)
            }),
            schema: serde_json::json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object"
            }),
            version: String::new(),
        };

        if i == 1 {
            backend.put(&key, &data, None).await?;
        } else {
            let current = backend.get(&key).await?;
            backend.put(&key, &data, Some(&current.version)).await?;
        }
    }

    // Verify we have 5 versions
    let versions = backend.list_versions(&key).await?;
    assert_eq!(versions.len(), 5);

    // Verify each version content
    for (i, version_info) in versions.iter().enumerate() {
        let data = backend.get_version(&key, &version_info.version).await?;
        let version_num = data.content["version"].as_i64().unwrap();
        assert_eq!(version_num as usize, i + 1);
    }

    Ok(())
}

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_s3_config_construction() {
    let config = StorageConfig::s3(
        "test-bucket",
        Some("us-east-1".to_string()),
        Some("http://localhost:9000".to_string()),
        Some("test-key".to_string()),
        Some("test-secret".to_string()),
        true,
    );

    match config {
        StorageConfig::S3 {
            bucket,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            allow_http,
        } => {
            assert_eq!(bucket, "test-bucket");
            assert_eq!(region, Some("us-east-1".to_string()));
            assert_eq!(endpoint, Some("http://localhost:9000".to_string()));
            assert_eq!(access_key_id, Some("test-key".to_string()));
            assert_eq!(secret_access_key, Some("test-secret".to_string()));
            assert_eq!(allow_http, true);
        }
        _ => panic!("Expected S3 config"),
    }
}

#[test]
fn test_local_config_construction() {
    let config = StorageConfig::local("./data");

    match config {
        StorageConfig::Local { path } => {
            assert_eq!(path.to_str().unwrap(), "./data");
        }
        _ => panic!("Expected Local config"),
    }
}
