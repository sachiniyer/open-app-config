use super::dto::*;
use super::handlers;
use super::state::AppState;
use crate::storage::{ObjectStoreBackend, StorageConfig};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{delete, get, put},
    Router,
};
use std::sync::Arc;
use tempfile::TempDir;
use tower::util::ServiceExt;

async fn create_test_app() -> (Router, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = StorageConfig::Local {
        path: temp_dir.path().to_path_buf(),
    };
    let storage = ObjectStoreBackend::from_config(config).unwrap();
    let state = Arc::new(AppState {
        storage: Arc::new(storage),
    });

    let app = Router::new()
        .route("/configs/:app/:env/:config", get(handlers::get_config))
        .route("/configs/:app/:env/:config", put(handlers::put_config))
        .route(
            "/configs/:app/:env/:config",
            delete(handlers::delete_config),
        )
        .route(
            "/configs/:app/:env/:config/versions",
            get(handlers::list_versions),
        )
        .route(
            "/configs/:app/:env/:config/versions/:version",
            get(handlers::get_config_version),
        )
        .route("/configs", get(handlers::list_configs))
        .route("/health", get(handlers::health_check))
        .with_state(state);

    (app, temp_dir)
}

#[tokio::test]
async fn test_health_check() {
    let (app, _dir) = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "healthy");
    assert_eq!(json["service"], "open-app-config");
    assert!(json.get("timestamp").is_some());
}

#[tokio::test]
async fn test_put_and_get_config() {
    let (app, _dir) = create_test_app().await;

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
                .body(Body::from(serde_json::to_string(&put_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Get the config
    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs/myapp/dev/database")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let config: GetConfigResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(config.application, "myapp");
    assert_eq!(config.environment, "dev");
    assert_eq!(config.config_name, "database");
    assert_eq!(config.version, "v1");
    assert_eq!(config.content, put_request.content);
    assert_eq!(config.schema, put_request.schema.unwrap());
}

#[tokio::test]
async fn test_update_config_with_optimistic_locking() {
    let (app, _dir) = create_test_app().await;

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
                .body(Body::from(serde_json::to_string(&put_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

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
                .body(Body::from(serde_json::to_string(&update_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

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
                .body(Body::from(serde_json::to_string(&bad_update).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_schema_required_for_first_version() {
    let (app, _dir) = create_test_app().await;

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
                .body(Body::from(serde_json::to_string(&put_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_versions() {
    let (app, _dir) = create_test_app().await;

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
                    .body(Body::from(serde_json::to_string(&put_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    // List versions
    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs/app/staging/multi/versions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let versions: ListVersionsResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(versions.versions.len(), 3);
    assert_eq!(versions.versions[0].version, "v1");
    assert_eq!(versions.versions[1].version, "v2");
    assert_eq!(versions.versions[2].version, "v3");
}

#[tokio::test]
async fn test_get_specific_version() {
    let (app, _dir) = create_test_app().await;

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
                .body(Body::from(serde_json::to_string(&put_request1).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

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
                .body(Body::from(serde_json::to_string(&put_request2).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Get v1
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/configs/app/dev/versioned/versions/v1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let config: GetConfigResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(config.content, v1_content);

    // Get v2
    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs/app/dev/versioned/versions/v2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let config: GetConfigResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(config.content, v2_content);
}

#[tokio::test]
async fn test_delete_config() {
    let (app, _dir) = create_test_app().await;

    // Create a config
    let put_request = PutConfigRequest {
        content: serde_json::json!({"temporary": true}),
        schema: Some(serde_json::json!({"type": "object"})),
        expected_version: None,
    };

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/configs/app/temp/config")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&put_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Delete it
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/configs/app/temp/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Try to get it - should fail
    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs/app/temp/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_list_configs() {
    let (app, _dir) = create_test_app().await;

    // Create several configs
    let configs = vec![
        ("app1", "dev", "db"),
        ("app1", "prod", "api"),
        ("app2", "dev", "cache"),
    ];

    for (app_name, env, config) in configs {
        let put_request = PutConfigRequest {
            content: serde_json::json!({"test": true}),
            schema: Some(serde_json::json!({"type": "object"})),
            expected_version: None,
        };

        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/configs/{}/{}/{}", app_name, env, config))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&put_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    // List all
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/configs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let list: ListConfigsResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(list.configs.len(), 3);

    // List with prefix
    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs?prefix=app1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let list: ListConfigsResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(list.configs.len(), 2);
    assert!(list.configs.iter().all(|c| c.application == "app1"));
}

#[tokio::test]
async fn test_get_nonexistent_config() {
    let (app, _dir) = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/configs/nonexistent/app/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
