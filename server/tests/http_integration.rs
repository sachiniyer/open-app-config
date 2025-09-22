use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{delete, get, put},
    Router,
};
use server::http::dto::*;
use server::http::handlers;
use server::http::state::AppState;
use server::storage::{ObjectStoreBackend, StorageConfig};
use std::sync::Arc;
use tempfile::TempDir;
use tower::util::ServiceExt;

fn create_test_app() -> anyhow::Result<(Router, TempDir)> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig::Local {
        path: temp_dir.path().to_path_buf(),
    };
    let storage = ObjectStoreBackend::from_config(config)?;
    let state = Arc::new(AppState {
        storage: Arc::new(storage),
    });

    let app = Router::new()
        .route("/configs/:app/:env/:config", get(handlers::get_config))
        .route("/configs/:app/:env/:config", put(handlers::put_config))
        .route("/configs/:app/:env", delete(handlers::delete_environment))
        .route(
            "/configs/:app/:env/:config/versions",
            get(handlers::list_versions),
        )
        .route(
            "/configs/:app/:env/:config/versions/:version",
            get(handlers::get_config_version),
        )
        .route("/health", get(handlers::health_check))
        .with_state(state);

    Ok((app, temp_dir))
}

#[tokio::test]
async fn test_health_check() -> anyhow::Result<()> {
    let (app, _dir) = create_test_app()?;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;

    assert_eq!(json["status"], "healthy");
    assert_eq!(json["service"], "open-app-config");
    assert!(json.get("timestamp").is_some());
    Ok(())
}

#[tokio::test]
async fn test_put_and_get_config() -> anyhow::Result<()> {
    let (app, _dir) = create_test_app()?;

    // Create a config
    let put_request = PutConfigRequest {
        content: serde_json::json!({"database": "postgres", "port": 5432}),
        schema: Some(serde_json::json!({"type": "object"})),
        expected_version: None,
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/configs/myapp/dev/database")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&put_request)?))?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    // Get the config
    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs/myapp/dev/database")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await?;
    let config: GetConfigResponse = serde_json::from_slice(&body)?;

    assert_eq!(config.application, "myapp");
    assert_eq!(config.environment, "dev");
    assert_eq!(config.config_name, "database");
    assert_eq!(config.version, "v1");
    assert_eq!(config.content, put_request.content);
    assert_eq!(config.schema, put_request.schema.ok_or(anyhow::anyhow!("missing schema"))?);
    Ok(())
}

#[tokio::test]
async fn test_update_config_with_optimistic_locking() -> anyhow::Result<()> {
    let (app, _dir) = create_test_app()?;

    // Create initial version
    let put_request = PutConfigRequest {
        content: serde_json::json!({"version": 1}),
        schema: Some(serde_json::json!({"type": "object"})),
        expected_version: None,
    };

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/configs/app/prod/api")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&put_request)?))?,
        )
        .await?;

    // Update with correct version
    let update_request = PutConfigRequest {
        content: serde_json::json!({"version": 2}),
        schema: None, // Use previous schema
        expected_version: Some("v1".to_string()),
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/configs/app/prod/api")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&update_request)?))?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    // Try to update with incorrect version - should fail
    let bad_update = PutConfigRequest {
        content: serde_json::json!({"version": 3}),
        schema: None,
        expected_version: Some("v1".to_string()), // Wrong version
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/configs/app/prod/api")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&bad_update)?))?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    Ok(())
}

#[tokio::test]
async fn test_schema_required_for_first_version() -> anyhow::Result<()> {
    let (app, _dir) = create_test_app()?;

    // Try to create without schema
    let put_request = PutConfigRequest {
        content: serde_json::json!({"test": true}),
        schema: None,
        expected_version: None,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/configs/app/dev/test")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&put_request)?))?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn test_list_versions() -> anyhow::Result<()> {
    let (app, _dir) = create_test_app()?;

    // Create multiple versions
    for i in 1..=3 {
        let put_request = PutConfigRequest {
            content: serde_json::json!({"version": i}),
            schema: if i == 1 {
                Some(serde_json::json!({"type": "object"}))
            } else {
                None
            },
            expected_version: if i == 1 {
                None
            } else {
                Some(format!("v{}", i - 1))
            },
        };

        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/configs/app/staging/multi")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&put_request)?))?,
            )
            .await?;
    }

    // List versions
    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs/app/staging/multi/versions")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await?;
    let versions: ListVersionsResponse = serde_json::from_slice(&body)?;

    assert_eq!(versions.versions.len(), 3);
    assert_eq!(versions.versions[0].version, "v1");
    assert_eq!(versions.versions[1].version, "v2");
    assert_eq!(versions.versions[2].version, "v3");
    Ok(())
}

#[tokio::test]
async fn test_get_specific_version() -> anyhow::Result<()> {
    let (app, _dir) = create_test_app()?;

    // Create two versions
    let v1_content = serde_json::json!({"feature": "a"});
    let v2_content = serde_json::json!({"feature": "b"});

    let put_request1 = PutConfigRequest {
        content: v1_content.clone(),
        schema: Some(serde_json::json!({"type": "object"})),
        expected_version: None,
    };

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/configs/app/dev/versioned")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&put_request1)?))?,
        )
        .await?;

    let put_request2 = PutConfigRequest {
        content: v2_content.clone(),
        schema: None,
        expected_version: Some("v1".to_string()),
    };

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/configs/app/dev/versioned")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&put_request2)?))?,
        )
        .await?;

    // Get v1
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/configs/app/dev/versioned/versions/v1")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await?;
    let config: GetConfigResponse = serde_json::from_slice(&body)?;
    assert_eq!(config.content, v1_content);

    // Get v2
    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs/app/dev/versioned/versions/v2")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await?;
    let config: GetConfigResponse = serde_json::from_slice(&body)?;
    assert_eq!(config.content, v2_content);
    Ok(())
}

#[tokio::test]
async fn test_delete_environment() -> anyhow::Result<()> {
    let (app, _dir) = create_test_app()?;

    // Create some configs in an environment
    let put_request = PutConfigRequest {
        content: serde_json::json!({"temporary": true}),
        schema: Some(serde_json::json!({"type": "object"})),
        expected_version: None,
    };

    // Create multiple configs
    for config_name in &["config1", "config2", "config3"] {
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/configs/app/temp/{config_name}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&put_request)?))?,
            )
            .await?;
    }

    // Delete the entire environment
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/configs/app/temp")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    // Try to get one of the configs - should fail
    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs/app/temp/config1")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn test_get_nonexistent_config() -> anyhow::Result<()> {
    let (app, _dir) = create_test_app()?;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs/nonexistent/app/config")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    Ok(())
}
