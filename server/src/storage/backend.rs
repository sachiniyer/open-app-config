use anyhow::{Context, Result};
use async_trait::async_trait;
use object_store::local::LocalFileSystem;
use object_store::path::Path;
use object_store::{ObjectStore, PutPayload};
use shared_types::{ConfigData, ConfigKey, VersionInfo};
use std::sync::Arc;

use super::config::StorageConfig;
use super::error::StorageError;
use super::metadata::Metadata;
use super::traits::ConfigStorage;

pub struct ObjectStoreBackend {
    store: Arc<dyn ObjectStore>,
}

impl ObjectStoreBackend {
    pub fn from_config(config: StorageConfig) -> Result<Self> {
        let store: Arc<dyn ObjectStore> = match config {
            StorageConfig::Local { path } => Arc::new(LocalFileSystem::new_with_prefix(path)?),
        };
        Ok(Self { store })
    }

    fn config_path(&self, key: &ConfigKey, file: &str) -> Path {
        Path::from(format!(
            "{}/{}/{}/{}",
            key.application, key.environment, key.config_name, file
        ))
    }

    fn version_path(&self, key: &ConfigKey, version: &str, file: &str) -> Path {
        Path::from(format!(
            "{}/{}/{}/versions/{}/{}",
            key.application, key.environment, key.config_name, version, file
        ))
    }

    async fn read_metadata(&self, key: &ConfigKey) -> Result<Option<Metadata>> {
        let path = self.config_path(key, "metadata.json");
        match self.store.get(&path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                let metadata: Metadata = serde_json::from_slice(&bytes)?;
                Ok(Some(metadata))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn write_metadata(&self, key: &ConfigKey, metadata: &Metadata) -> Result<()> {
        let path = self.config_path(key, "metadata.json");
        let json = serde_json::to_vec_pretty(metadata)?;
        self.store.put(&path, PutPayload::from(json)).await?;
        Ok(())
    }
}

#[async_trait]
impl ConfigStorage for ObjectStoreBackend {
    async fn put(
        &self,
        key: &ConfigKey,
        data: &ConfigData,
        expected_version: Option<&str>,
    ) -> Result<()> {
        let existing_metadata = self.read_metadata(key).await?;

        match (&existing_metadata, expected_version) {
            (None, None) => {}
            (Some(m), Some(expected)) if m.current_version == expected => {}
            (None, Some(expected)) => {
                return Err(StorageError::VersionConflict {
                    expected: expected.to_string(),
                    actual: "none".to_string(),
                }
                .into());
            }
            (Some(_), None) => {
                return Err(StorageError::AlreadyExists(format!(
                    "Configuration {key} already exists. Use expected_version to update."
                ))
                .into());
            }
            (Some(m), Some(expected)) => {
                return Err(StorageError::VersionConflict {
                    expected: expected.to_string(),
                    actual: m.current_version.clone(),
                }
                .into());
            }
        }

        let mut metadata = existing_metadata.unwrap_or_else(Metadata::new);
        let version = format!("v{}", metadata.next_version_number());

        let data_path = self.version_path(key, &version, "data.json");
        let data_json = serde_json::to_vec_pretty(&data.content)?;
        self.store
            .put(&data_path, PutPayload::from(data_json))
            .await?;

        let schema_path = self.version_path(key, &version, "schema.json");
        let schema_json = serde_json::to_vec_pretty(&data.schema)?;
        self.store
            .put(&schema_path, PutPayload::from(schema_json))
            .await?;

        metadata.add_version(version);
        self.write_metadata(key, &metadata).await?;

        Ok(())
    }

    async fn get(&self, key: &ConfigKey) -> Result<ConfigData> {
        let metadata = self
            .read_metadata(key)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("Config not found: {key}")))?;

        if metadata.current_version.is_empty() {
            return Err(
                StorageError::NotFound(format!("No versions found for config: {key}")).into(),
            );
        }

        self.get_version(key, &metadata.current_version).await
    }

    async fn get_version(&self, key: &ConfigKey, version: &str) -> Result<ConfigData> {
        let data_path = self.version_path(key, version, "data.json");
        let data_result = self
            .store
            .get(&data_path)
            .await
            .with_context(|| format!("Failed to read data for {key} @ {version}"))?;
        let content: serde_json::Value = serde_json::from_slice(&data_result.bytes().await?)?;

        let schema_path = self.version_path(key, version, "schema.json");
        let schema_result = self
            .store
            .get(&schema_path)
            .await
            .with_context(|| format!("Failed to read schema for {key} @ {version}"))?;
        let schema: serde_json::Value = serde_json::from_slice(&schema_result.bytes().await?)?;

        Ok(ConfigData {
            content,
            schema,
            version: version.to_string(),
        })
    }

    async fn delete_environment(&self, app: &str, env: &str) -> Result<usize> {
        use futures::StreamExt;

        // List all files in the app/env prefix
        let prefix = Path::from(format!("{app}/{env}"));
        let mut stream = self.store.list(Some(&prefix));

        let mut deleted_count = 0;
        let mut configs_found = std::collections::HashSet::new();

        // Find all unique config names
        while let Some(meta) = stream.next().await.transpose()? {
            let parts: Vec<_> = meta.location.parts().collect();
            if parts.len() >= 3 {
                configs_found.insert(parts[2].as_ref().to_string());
            }
        }

        // Delete each config
        for config_name in configs_found {
            let key = ConfigKey::new(app.to_string(), env.to_string(), config_name);

            let metadata_result = self.read_metadata(&key).await;
            #[allow(clippy::single_match)]
            match metadata_result {
                Ok(Some(metadata)) => {
                    // Delete all version files
                    for version_meta in &metadata.versions {
                        let data_path = self.version_path(&key, &version_meta.version, "data.json");
                        let _ = self.store.delete(&data_path).await;
                        let schema_path =
                            self.version_path(&key, &version_meta.version, "schema.json");
                        let _ = self.store.delete(&schema_path).await;
                    }

                    // Delete metadata
                    let metadata_path = self.config_path(&key, "metadata.json");
                    let _ = self.store.delete(&metadata_path).await;

                    deleted_count += 1;
                }
                _ => {}
            }
        }

        Ok(deleted_count)
    }

    async fn exists(&self, key: &ConfigKey) -> Result<bool> {
        let path = self.config_path(key, "metadata.json");
        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn list_versions(&self, key: &ConfigKey) -> Result<Vec<VersionInfo>> {
        let metadata = self
            .read_metadata(key)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("Config not found: {key}")))?;

        Ok(metadata
            .versions
            .iter()
            .map(|v| VersionInfo {
                version: v.version.clone(),
                timestamp: v.timestamp,
            })
            .collect())
    }
}
