use axum::{
    extract::{Path, State},
    Json,
};
use shared_types::ConfigKey;
use std::sync::Arc;
use tracing::{info, instrument};

use super::{
    dto::{GetConfigResponse, ListVersionsResponse, PutConfigRequest, SuccessResponse},
    error::ApiResult,
    state::AppState,
};

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
        .map_err(|e| super::error::ApiError::NotFound(format!("Config not found: {e}")))?;

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
        .map_err(|e| super::error::ApiError::NotFound(format!("Config not found: {e}")))?;

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
            super::error::ApiError::NotFound(format!("Config version not found: {e}"))
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

    let schema = resolve_schema(&state, &key, &request).await?;
    validate_request(&request, &schema)?;

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
        message: format!("Configuration {key} updated successfully"),
        version: Some(format!(
            "v{}",
            state
                .storage
                .get(&key)
                .await
                .map_or_else(|_| "unknown".to_string(), |d| d.version)
        )),
    }))
}

fn validate_request(request: &PutConfigRequest, schema: &serde_json::Value) -> ApiResult<()> {
    if !request.content.is_object() {
        return Err(super::error::ApiError::BadRequest(
            "Content must be a JSON object".to_string(),
        ));
    }

    // Validate content against schema
    let compiled_schema = jsonschema::Validator::new(schema)
        .map_err(|e| super::error::ApiError::BadRequest(format!("Invalid schema: {e}")))?;

    let validation_result = compiled_schema.validate(&request.content);
    if let Err(errors) = validation_result {
        let error_messages: Vec<String> = errors
            .map(|e| format!("{}: {}", e.instance_path, e))
            .collect();
        return Err(super::error::ApiError::BadRequest(format!(
            "Validation failed: {}",
            error_messages.join(", ")
        )));
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
                    "Failed to fetch previous version: {e}"
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
                    "Failed to fetch current version: {e}"
                ))
            });
    }

    Err(super::error::ApiError::BadRequest(
        "Schema is required when creating the first version".to_string(),
    ))
}

/// DELETE /configs/:app/:env
/// Delete all configurations for an application environment
#[instrument(skip(state))]
pub async fn delete_environment(
    State(state): State<Arc<AppState>>,
    Path((app, env)): Path<(String, String)>,
) -> ApiResult<Json<SuccessResponse>> {
    info!("Deleting all configs for: {}/{}", app, env);

    let deleted_count = state
        .storage
        .delete_environment(&app, &env)
        .await
        .map_err(|e| {
            super::error::ApiError::InternalError(format!("Failed to delete environment: {e}"))
        })?;

    Ok(Json(SuccessResponse {
        message: format!(
            "Deleted {deleted_count} configurations for {app}/{env}"
        ),
        version: None,
    }))
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
