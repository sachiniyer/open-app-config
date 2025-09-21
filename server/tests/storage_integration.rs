#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use server::storage::{ConfigStorage, ObjectStoreBackend, StorageConfig};
use shared_types::{ConfigData, ConfigKey};
use tempfile::TempDir;

fn create_test_backend() -> (ObjectStoreBackend, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = StorageConfig::Local {
        path: temp_dir.path().to_path_buf(),
    };
    let backend = ObjectStoreBackend::from_config(config).unwrap();
    (backend, temp_dir)
}

#[tokio::test]
async fn test_put_and_get_config() {
    let (backend, _dir) = create_test_backend();

    let key = ConfigKey::new("test-app", "dev", "database");
    let data = ConfigData {
        content: serde_json::json!({"host": "localhost", "port": 5432}),
        schema: serde_json::json!({"type": "object"}),
        version: String::new(),
    };

    // First put should succeed with no expected version
    backend.put(&key, &data, None).await.unwrap();

    // Get should return the data
    let retrieved = backend.get(&key).await.unwrap();
    assert_eq!(retrieved.content, data.content);
    assert_eq!(retrieved.schema, data.schema);
    assert_eq!(retrieved.version, "v1");
}

#[tokio::test]
async fn test_optimistic_concurrency_control() {
    let (backend, _dir) = create_test_backend();

    let key = ConfigKey::new("test-app", "prod", "api");
    let data1 = ConfigData {
        content: serde_json::json!({"version": 1}),
        schema: serde_json::json!({"type": "object"}),
        version: String::new(),
    };

    // Create first version
    backend.put(&key, &data1, None).await.unwrap();

    // Update with correct version should succeed
    let data2 = ConfigData {
        content: serde_json::json!({"version": 2}),
        schema: serde_json::json!({"type": "object"}),
        version: String::new(),
    };
    backend.put(&key, &data2, Some("v1")).await.unwrap();

    // Update with incorrect version should fail
    let data3 = ConfigData {
        content: serde_json::json!({"version": 3}),
        schema: serde_json::json!({"type": "object"}),
        version: String::new(),
    };
    let result = backend.put(&key, &data3, Some("v1")).await;
    assert!(result.is_err());

    // Update with no version when one exists should fail
    let result = backend.put(&key, &data3, None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_list_versions() {
    let (backend, _dir) = create_test_backend();

    let key = ConfigKey::new("test-app", "staging", "cache");

    // Create multiple versions
    for i in 1..=3 {
        let data = ConfigData {
            content: serde_json::json!({"version": i}),
            schema: serde_json::json!({"type": "object"}),
            version: String::new(),
        };

        let expected_version = if i == 1 {
            None
        } else {
            Some(format!("v{}", i - 1))
        };

        backend
            .put(&key, &data, expected_version.as_deref())
            .await
            .unwrap();
    }

    // List versions
    let versions = backend.list_versions(&key).await.unwrap();
    assert_eq!(versions.len(), 3);
    assert_eq!(versions[0].version, "v1");
    assert_eq!(versions[1].version, "v2");
    assert_eq!(versions[2].version, "v3");
}

#[tokio::test]
async fn test_get_specific_version() {
    let (backend, _dir) = create_test_backend();

    let key = ConfigKey::new("test-app", "dev", "features");

    // Create two versions
    let data1 = ConfigData {
        content: serde_json::json!({"feature": "a"}),
        schema: serde_json::json!({"type": "object"}),
        version: String::new(),
    };
    backend.put(&key, &data1, None).await.unwrap();

    let data2 = ConfigData {
        content: serde_json::json!({"feature": "b"}),
        schema: serde_json::json!({"type": "object", "required": ["feature"]}),
        version: String::new(),
    };
    backend.put(&key, &data2, Some("v1")).await.unwrap();

    // Get specific versions
    let v1 = backend.get_version(&key, "v1").await.unwrap();
    assert_eq!(v1.content, data1.content);
    assert_eq!(v1.schema, data1.schema);

    let v2 = backend.get_version(&key, "v2").await.unwrap();
    assert_eq!(v2.content, data2.content);
    assert_eq!(v2.schema, data2.schema);

    // Get current version
    let current = backend.get(&key).await.unwrap();
    assert_eq!(current.version, "v2");
    assert_eq!(current.content, data2.content);
}

#[tokio::test]
async fn test_delete_environment() {
    let (backend, _dir) = create_test_backend();

    // Create multiple configs in same environment
    let configs = vec![
        ConfigKey::new("test-app", "temp", "config1"),
        ConfigKey::new("test-app", "temp", "config2"),
        ConfigKey::new("test-app", "temp", "config3"),
    ];

    for key in &configs {
        let data = ConfigData {
            content: serde_json::json!({"temp": true}),
            schema: serde_json::json!({"type": "object"}),
            version: String::new(),
        };
        backend.put(key, &data, None).await.unwrap();
    }

    // Verify all exist
    for key in &configs {
        assert!(backend.exists(key).await.unwrap());
    }

    // Delete entire environment
    let deleted = backend
        .delete_environment("test-app", "temp")
        .await
        .unwrap();
    assert_eq!(deleted, 3);

    // Verify all are gone
    for key in &configs {
        assert!(!backend.exists(key).await.unwrap());
    }
}

#[tokio::test]
async fn test_get_nonexistent_config() {
    let (backend, _dir) = create_test_backend();

    let key = ConfigKey::new("nonexistent", "app", "config");
    let result = backend.get(&key).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_nonexistent_version() {
    let (backend, _dir) = create_test_backend();

    let key = ConfigKey::new("test-app", "dev", "config");
    let data = ConfigData {
        content: serde_json::json!({"test": true}),
        schema: serde_json::json!({"type": "object"}),
        version: String::new(),
    };

    backend.put(&key, &data, None).await.unwrap();

    let result = backend.get_version(&key, "v999").await;
    assert!(result.is_err());
}
