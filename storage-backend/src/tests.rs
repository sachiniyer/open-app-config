#[cfg(test)]
mod tests {
    use crate::backend::ObjectStoreBackend;
    use crate::config::StorageConfig;
    use crate::ConfigStorage;
    use shared_types::{ConfigData, ConfigKey};
    use tempfile::TempDir;

    async fn setup_test_backend() -> (ObjectStoreBackend, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig::local(temp_dir.path());
        let backend = ObjectStoreBackend::from_config(config).unwrap();
        (backend, temp_dir)
    }

    fn create_test_key(name: &str) -> ConfigKey {
        ConfigKey::new("test-app", "test-env", name)
    }

    fn create_test_data(content: serde_json::Value, version: &str) -> ConfigData {
        ConfigData {
            content,
            schema: Some(serde_json::json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "value": {"type": "number"}
                }
            })),
            version: version.to_string(),
        }
    }

    #[tokio::test]
    async fn test_put_and_get() {
        let (backend, _temp) = setup_test_backend().await;
        let key = create_test_key("test-config");
        let data = create_test_data(
            serde_json::json!({
                "name": "test",
                "value": 42
            }),
            "v1",
        );

        // Put config (first creation, no expected version)
        backend.put(&key, &data, None).await.unwrap();

        // Get config
        let retrieved = backend.get(&key).await.unwrap();
        assert_eq!(retrieved.content, data.content);
        assert_eq!(retrieved.schema, data.schema);
        assert_eq!(retrieved.version, "v1");
    }

    #[tokio::test]
    async fn test_versioning() {
        let (backend, _temp) = setup_test_backend().await;
        let key = create_test_key("versioned-config");

        // Store v1
        let data_v1 = create_test_data(serde_json::json!({"version": "1.0.0", "value": 1}), "v1");
        backend.put(&key, &data_v1, None).await.unwrap();

        // Store v2
        let data_v2 = create_test_data(serde_json::json!({"version": "2.0.0", "value": 2}), "v2");
        backend.put(&key, &data_v2, Some("v1")).await.unwrap();

        // Store v3
        let data_v3 = create_test_data(serde_json::json!({"version": "3.0.0", "value": 3}), "v3");
        backend.put(&key, &data_v3, Some("v2")).await.unwrap();

        // Current version should be v3
        let current = backend.get(&key).await.unwrap();
        assert_eq!(current.version, "v3");
        assert_eq!(current.content["value"], 3);

        // Get specific versions
        let v1 = backend.get_version(&key, "v1").await.unwrap();
        assert_eq!(v1.content["value"], 1);

        let v2 = backend.get_version(&key, "v2").await.unwrap();
        assert_eq!(v2.content["value"], 2);

        // List versions
        let versions = backend.list_versions(&key).await.unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version, "v1");
        assert_eq!(versions[1].version, "v2");
        assert_eq!(versions[2].version, "v3");
    }

    #[tokio::test]
    async fn test_exists() {
        let (backend, _temp) = setup_test_backend().await;
        let key = create_test_key("exists-test");

        // Should not exist initially
        assert!(!backend.exists(&key).await.unwrap());

        // Store config
        let data = create_test_data(serde_json::json!({"test": true}), "v1");
        backend.put(&key, &data, None).await.unwrap();

        // Should exist now
        assert!(backend.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let (backend, _temp) = setup_test_backend().await;
        let key = create_test_key("delete-test");

        // Store config with multiple versions
        let mut last_version = None;
        for i in 1..=3 {
            let data = create_test_data(serde_json::json!({"iteration": i}), &format!("v{}", i));
            backend
                .put(&key, &data, last_version.as_deref())
                .await
                .unwrap();
            last_version = Some(format!("v{}", i));
        }

        // Verify it exists
        assert!(backend.exists(&key).await.unwrap());

        // Delete
        backend.delete(&key).await.unwrap();

        // Should not exist anymore
        assert!(!backend.exists(&key).await.unwrap());

        // Getting should fail
        assert!(backend.get(&key).await.is_err());
    }

    #[tokio::test]
    async fn test_list() {
        let (backend, _temp) = setup_test_backend().await;

        // Store multiple configs
        for i in 1..=3 {
            let key = ConfigKey::new("app1", "prod", &format!("config{}", i));
            let data = create_test_data(serde_json::json!({"id": i}), "v1");
            backend.put(&key, &data, None).await.unwrap();
        }

        for i in 1..=2 {
            let key = ConfigKey::new("app2", "staging", &format!("config{}", i));
            let data = create_test_data(serde_json::json!({"id": i}), "v1");
            backend.put(&key, &data, None).await.unwrap();
        }

        // List all
        let all_configs = backend.list(None).await.unwrap();
        assert_eq!(all_configs.len(), 5);

        // List with prefix
        let app1_configs = backend.list(Some("app1")).await.unwrap();
        assert_eq!(app1_configs.len(), 3);

        let app2_configs = backend.list(Some("app2")).await.unwrap();
        assert_eq!(app2_configs.len(), 2);
    }

    #[tokio::test]
    async fn test_config_without_schema() {
        let (backend, _temp) = setup_test_backend().await;
        let key = create_test_key("no-schema");

        // Create config without schema
        let data = ConfigData {
            content: serde_json::json!({"simple": "data"}),
            schema: None,
            version: "v1".to_string(),
        };

        backend.put(&key, &data, None).await.unwrap();

        // Retrieve and verify
        let retrieved = backend.get(&key).await.unwrap();
        assert_eq!(retrieved.content, data.content);
        assert!(retrieved.schema.is_none());
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let (backend, _temp) = setup_test_backend().await;
        let key = create_test_key("nonexistent");

        let result = backend.get(&key).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_get_version_nonexistent() {
        let (backend, _temp) = setup_test_backend().await;
        let key = create_test_key("version-test");

        // Store v1
        let data = create_test_data(serde_json::json!({"test": 1}), "v1");
        backend.put(&key, &data, None).await.unwrap();

        // Try to get non-existent version
        let result = backend.get_version(&key, "v99").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Version v99 not found"));
    }

    #[tokio::test]
    async fn test_version_conflict() {
        let (backend, _temp) = setup_test_backend().await;
        let key = create_test_key("conflict-test");

        // Create initial version
        let data_v1 = create_test_data(serde_json::json!({"value": 1}), "v1");
        backend.put(&key, &data_v1, None).await.unwrap();

        // Try to create again (should fail - already exists)
        let data_v2 = create_test_data(serde_json::json!({"value": 2}), "v2");
        let result = backend.put(&key, &data_v2, None).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Version conflict"));
        assert!(err.contains("expected none, but found v1"));

        // Try to update with wrong version (should fail)
        let result = backend.put(&key, &data_v2, Some("v99")).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Version conflict"));
        assert!(err.contains("expected v99, but found v1"));

        // Update with correct version (should succeed)
        backend.put(&key, &data_v2, Some("v1")).await.unwrap();

        // Verify update worked
        let current = backend.get(&key).await.unwrap();
        assert_eq!(current.version, "v2");
        assert_eq!(current.content["value"], 2);
    }
}
