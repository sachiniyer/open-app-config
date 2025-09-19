use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub current_version: String,
    pub versions: Vec<VersionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMetadata {
    pub version: String,
    pub timestamp: DateTime<Utc>,
    pub data_size: usize,
    pub has_schema: bool,
}

impl Metadata {
    pub fn new() -> Self {
        Self {
            current_version: String::new(),
            versions: Vec::new(),
        }
    }

    pub fn add_version(&mut self, version: String, data_size: usize, has_schema: bool) {
        let version_meta = VersionMetadata {
            version: version.clone(),
            timestamp: Utc::now(),
            data_size,
            has_schema,
        };
        self.versions.push(version_meta);
        self.current_version = version;
    }

    pub fn next_version_number(&self) -> u32 {
        self.versions
            .iter()
            .filter_map(|v| {
                v.version
                    .strip_prefix('v')
                    .and_then(|n| n.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0)
            + 1
    }
}
