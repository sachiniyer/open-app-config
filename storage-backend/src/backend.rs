use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use object_store::local::LocalFileSystem;
use object_store::path::Path;
use object_store::{ObjectStore, PutPayload};
use shared_types::{ConfigData, ConfigKey, VersionInfo};
use std::sync::Arc;
use tracing::{debug, info, instrument};

use crate::config::StorageConfig;
use crate::metadata::Metadata;
use crate::ConfigStorage;

pub struct ObjectStoreBackend {
    store: Arc<dyn ObjectStore>,
}

impl ObjectStoreBackend {
    pub fn from_config(config: StorageConfig) -> Result<Self> {
        let store: Arc<dyn ObjectStore> = match config {
            StorageConfig::Local { path } => {
                info!("Initializing local storage at: {:?}", path);
                Arc::new(LocalFileSystem::new_with_prefix(path)?)
            }
        };

        Ok(Self { store })
    }

    fn config_base_path(&self, key: &ConfigKey) -> Path {
        Path::from(format!(
            "{}/{}/{}",
            key.application, key.environment, key.config_name
        ))
    }

    fn metadata_path(&self, key: &ConfigKey) -> Path {
        self.config_base_path(key).child("metadata.json")
    }

    fn version_data_path(&self, key: &ConfigKey, version: &str) -> Path {
        self.config_base_path(key)
            .child("versions")
            .child(version)
            .child("data.json")
    }

    fn version_schema_path(&self, key: &ConfigKey, version: &str) -> Path {
        self.config_base_path(key)
            .child("versions")
            .child(version)
            .child("schema.json")
    }

    async fn read_metadata(&self, key: &ConfigKey) -> Result<Option<Metadata>> {
        let path = self.metadata_path(key);

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
        let path = self.metadata_path(key);
        let json = serde_json::to_vec_pretty(metadata)?;
        let payload = PutPayload::from(json);

        self.store.put(&path, payload).await?;
        Ok(())
    }
}

#[async_trait]
impl ConfigStorage for ObjectStoreBackend {
    #[instrument(skip(self, data))]
    async fn put(&self, key: &ConfigKey, data: &ConfigData) -> Result<()> {
        debug!("Storing config for key: {}", key);

        // Read existing metadata or create new
        let mut metadata = self.read_metadata(key).await?.unwrap_or_else(Metadata::new);

        // Generate next version
        let version_num = metadata.next_version_number();
        let version = format!("v{}", version_num);

        // Write data.json
        let data_path = self.version_data_path(key, &version);
        let data_json = serde_json::to_vec_pretty(&data.content)?;
        let data_payload = PutPayload::from(data_json.clone());
        self.store.put(&data_path, data_payload).await?;

        // Write schema.json if present
        let has_schema = if let Some(ref schema) = data.schema {
            let schema_path = self.version_schema_path(key, &version);
            let schema_json = serde_json::to_vec_pretty(schema)?;
            let schema_payload = PutPayload::from(schema_json);
            self.store.put(&schema_path, schema_payload).await?;
            true
        } else {
            false
        };

        // Update metadata
        metadata.add_version(version.clone(), data_json.len(), has_schema);
        self.write_metadata(key, &metadata).await?;

        info!("Stored config {} as version {}", key, version);
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get(&self, key: &ConfigKey) -> Result<ConfigData> {
        debug!("Getting config for key: {}", key);

        // Read metadata
        let metadata = self
            .read_metadata(key)
            .await?
            .ok_or_else(|| anyhow!("Config not found: {}", key))?;

        if metadata.current_version.is_empty() {
            bail!("No versions found for config: {}", key);
        }

        // Read current version data
        let data_path = self.version_data_path(key, &metadata.current_version);
        let data_result = self
            .store
            .get(&data_path)
            .await
            .with_context(|| format!("Failed to read data for {}", key))?;
        let data_bytes = data_result.bytes().await?;
        let content: serde_json::Value = serde_json::from_slice(&data_bytes)?;

        // Read schema if it exists
        let schema = if metadata
            .versions
            .iter()
            .find(|v| v.version == metadata.current_version)
            .map(|v| v.has_schema)
            .unwrap_or(false)
        {
            let schema_path = self.version_schema_path(key, &metadata.current_version);
            match self.store.get(&schema_path).await {
                Ok(result) => {
                    let bytes = result.bytes().await?;
                    Some(serde_json::from_slice(&bytes)?)
                }
                Err(_) => None,
            }
        } else {
            None
        };

        Ok(ConfigData {
            content,
            schema,
            version: metadata.current_version,
        })
    }

    #[instrument(skip(self))]
    async fn get_version(&self, key: &ConfigKey, version: &str) -> Result<ConfigData> {
        debug!("Getting version {} for key: {}", version, key);

        // Verify version exists in metadata
        let metadata = self
            .read_metadata(key)
            .await?
            .ok_or_else(|| anyhow!("Config not found: {}", key))?;

        let version_meta = metadata
            .versions
            .iter()
            .find(|v| v.version == version)
            .ok_or_else(|| anyhow!("Version {} not found for {}", version, key))?;

        // Read version data
        let data_path = self.version_data_path(key, version);
        let data_result = self.store.get(&data_path).await?;
        let data_bytes = data_result.bytes().await?;
        let content: serde_json::Value = serde_json::from_slice(&data_bytes)?;

        // Read schema if it exists
        let schema = if version_meta.has_schema {
            let schema_path = self.version_schema_path(key, version);
            match self.store.get(&schema_path).await {
                Ok(result) => {
                    let bytes = result.bytes().await?;
                    Some(serde_json::from_slice(&bytes)?)
                }
                Err(_) => None,
            }
        } else {
            None
        };

        Ok(ConfigData {
            content,
            schema,
            version: version.to_string(),
        })
    }

    #[instrument(skip(self))]
    async fn delete(&self, key: &ConfigKey) -> Result<()> {
        debug!("Deleting config: {}", key);

        // Get metadata to know what versions to delete
        let metadata = self
            .read_metadata(key)
            .await?
            .ok_or_else(|| anyhow!("Config not found: {}", key))?;

        // Delete all version files
        for version_meta in &metadata.versions {
            let data_path = self.version_data_path(key, &version_meta.version);
            self.store.delete(&data_path).await?;

            if version_meta.has_schema {
                let schema_path = self.version_schema_path(key, &version_meta.version);
                let _ = self.store.delete(&schema_path).await;
            }
        }

        // Delete metadata
        let metadata_path = self.metadata_path(key);
        self.store.delete(&metadata_path).await?;

        info!("Deleted config: {}", key);
        Ok(())
    }

    #[instrument(skip(self))]
    async fn exists(&self, key: &ConfigKey) -> Result<bool> {
        let path = self.metadata_path(key);
        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    #[instrument(skip(self))]
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<ConfigKey>> {
        debug!("Listing configs with prefix: {:?}", prefix);

        let list_prefix = prefix.map(Path::from).unwrap_or_else(|| Path::from("/"));

        let mut keys = Vec::new();
        let mut stream = self.store.list(Some(&list_prefix));

        while let Some(meta) = stream.next().await.transpose()? {
            // Parse metadata.json paths back to ConfigKey
            if meta.location.filename() == Some("metadata.json") {
                let parts: Vec<_> = meta.location.parts().collect();
                if parts.len() >= 4 {
                    // Path format: app/env/config/metadata.json
                    let key = ConfigKey {
                        application: parts[0].as_ref().to_string(),
                        environment: parts[1].as_ref().to_string(),
                        config_name: parts[2].as_ref().to_string(),
                    };
                    keys.push(key);
                }
            }
        }

        Ok(keys)
    }

    #[instrument(skip(self))]
    async fn list_versions(&self, key: &ConfigKey) -> Result<Vec<VersionInfo>> {
        debug!("Listing versions for key: {}", key);

        let metadata = self
            .read_metadata(key)
            .await?
            .ok_or_else(|| anyhow!("Config not found: {}", key))?;

        let versions = metadata
            .versions
            .iter()
            .map(|v| VersionInfo {
                version: v.version.clone(),
                timestamp: v.timestamp,
            })
            .collect();

        Ok(versions)
    }
}
