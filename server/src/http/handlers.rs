#![allow(clippy::uninlined_format_args)]
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use shared_types::ConfigKey;
use std::sync::Arc;
use tracing::{info, instrument};

use super::{
    dto::{
        ConfigSummary, GetConfigResponse, ListConfigsResponse, ListVersionsResponse,
        PutConfigRequest, SuccessResponse,
    },
    error::ApiResult,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub prefix: Option<String>,
}

/// GET /configs/:app/:env/:config
/// Get the current version of a configuration
#[instrument(skip(state))]
pub async fn get_config(
    State(state): State<Arc<AppState>>,
    Path((app, env, config)): Path<(String, String, String)>,
) -> ApiResult<Json<GetConfigResponse>> {
    info!("Getting config: {}/{}/{}", app, env, config);

    let key = ConfigKey::new(app, env, config);

    let data = state
        .storage
        .get(&key)
        .await
        .map_err(|e| super::error::ApiError::NotFound(format!("Config not found: {}", e)))?;

    Ok(Json(GetConfigResponse::from_data_and_key(data, &key)))
}

/// GET /configs/:app/:env/:config/versions
/// List all versions of a configuration
#[instrument(skip(state))]
pub async fn list_versions(
    State(state): State<Arc<AppState>>,
    Path((app, env, config)): Path<(String, String, String)>,
) -> ApiResult<Json<ListVersionsResponse>> {
    info!("Listing versions for: {}/{}/{}", app, env, config);

    let key = ConfigKey::new(app, env, config);

    let versions = state
        .storage
        .list_versions(&key)
        .await
        .map_err(|e| super::error::ApiError::NotFound(format!("Config not found: {}", e)))?;

    Ok(Json(ListVersionsResponse { versions }))
}

/// GET /configs/:app/:env/:config/versions/:version
/// Get a specific version of a configuration
#[instrument(skip(state))]
pub async fn get_config_version(
    State(state): State<Arc<AppState>>,
    Path((app, env, config, version)): Path<(String, String, String, String)>,
) -> ApiResult<Json<GetConfigResponse>> {
    info!(
        "Getting config version: {}/{}/{} @ {}",
        app, env, config, version
    );

    let key = ConfigKey::new(app, env, config);

    let data = state
        .storage
        .get_version(&key, &version)
        .await
        .map_err(|e| {
            super::error::ApiError::NotFound(format!("Config version not found: {}", e))
        })?;

    Ok(Json(GetConfigResponse::from_data_and_key(data, &key)))
}

/// PUT /configs/:app/:env/:config
#[instrument(skip(state, request))]
pub async fn put_config(
    State(state): State<Arc<AppState>>,
    Path((app, env, config)): Path<(String, String, String)>,
    Json(request): Json<PutConfigRequest>,
) -> ApiResult<Json<SuccessResponse>> {
    info!("Putting config: {}/{}/{}", app, env, config);
    let key = ConfigKey::new(app, env, config);

    validate_request(&request)?;
    let schema = resolve_schema(&state, &key, &request).await?;

    let config_data = shared_types::ConfigData {
        content: request.content,
        schema,
        version: String::new(),
    };

    state
        .storage
        .put(&key, &config_data, request.expected_version.as_deref())
        .await
        .map_err(|e| super::error::ApiError::InternalError(e.to_string()))?;

    Ok(Json(SuccessResponse {
        message: format!("Configuration {} updated successfully", key),
        version: Some(format!(
            "v{}",
            state
                .storage
                .get(&key)
                .await
                .map(|d| d.version)
                .unwrap_or_else(|_| "unknown".to_string())
        )),
    }))
}

fn validate_request(request: &PutConfigRequest) -> ApiResult<()> {
    if !request.content.is_object() {
        return Err(super::error::ApiError::BadRequest(
            "Content must be a JSON object".to_string(),
        ));
    }
    Ok(())
}

async fn resolve_schema(
    state: &Arc<AppState>,
    key: &ConfigKey,
    request: &PutConfigRequest,
) -> ApiResult<serde_json::Value> {
    if let Some(schema) = &request.schema {
        if !schema.is_object() {
            return Err(super::error::ApiError::BadRequest(
                "Schema must be a valid JSON Schema object".to_string(),
            ));
        }
        return Ok(schema.clone());
    }

    if let Some(version) = &request.expected_version {
        return state
            .storage
            .get_version(key, version)
            .await
            .map(|data| data.schema)
            .map_err(|e| {
                super::error::ApiError::InternalError(format!(
                    "Failed to fetch previous version: {}",
                    e
                ))
            });
    }

    if state.storage.exists(key).await.unwrap_or(false) {
        return state
            .storage
            .get(key)
            .await
            .map(|data| data.schema)
            .map_err(|e| {
                super::error::ApiError::InternalError(format!(
                    "Failed to fetch current version: {}",
                    e
                ))
            });
    }

    Err(super::error::ApiError::BadRequest(
        "Schema is required when creating the first version".to_string(),
    ))
}

/// DELETE /configs/:app/:env/:config
/// Delete a configuration and all its versions
#[instrument(skip(state))]
pub async fn delete_config(
    State(state): State<Arc<AppState>>,
    Path((app, env, config)): Path<(String, String, String)>,
) -> ApiResult<Json<SuccessResponse>> {
    info!("Deleting config: {}/{}/{}", app, env, config);

    let key = ConfigKey::new(app, env, config);

    // Check if config exists before trying to delete
    let exists = state.storage.exists(&key).await.map_err(|e| {
        super::error::ApiError::InternalError(format!("Failed to check existence: {}", e))
    })?;

    if !exists {
        return Err(super::error::ApiError::NotFound(format!(
            "Configuration {} not found",
            key
        )));
    }

    state.storage.delete(&key).await.map_err(|e| {
        super::error::ApiError::InternalError(format!("Failed to delete config: {}", e))
    })?;

    Ok(Json(SuccessResponse {
        message: format!("Configuration {} deleted successfully", key),
        version: None,
    }))
}

/// GET /configs
/// List all configurations with optional prefix filtering
#[instrument(skip(state))]
pub async fn list_configs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<ListConfigsResponse>> {
    info!("Listing configs with prefix: {:?}", params.prefix);

    let keys = state
        .storage
        .list(params.prefix.as_deref())
        .await
        .map_err(|e| {
            super::error::ApiError::InternalError(format!("Failed to list configs: {}", e))
        })?;

    // Convert keys to config summaries
    let mut configs = Vec::new();
    for key in keys {
        // Get the current version for each config
        let result = state.storage.get(&key).await;
        match result {
            Ok(data) => {
                configs.push(ConfigSummary {
                    application: key.application,
                    environment: key.environment,
                    config_name: key.config_name,
                    current_version: data.version,
                });
            }
            Err(e) => {
                // Log error but continue with other configs
                tracing::warn!("Failed to get config {}: {}", key, e);
            }
        }
    }

    Ok(Json(ListConfigsResponse { configs }))
}

/// GET /health
/// Health check endpoint
pub async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "open-app-config",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}
