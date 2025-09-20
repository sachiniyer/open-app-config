use client::ConfigClient;
use mockito::{self, Matcher};
use serde_json::json;
use shared_types::ConfigKey;

#[tokio::test]
async fn test_health_check() {
    let mut server = mockito::Server::new_async().await;

    let _m = server
        .mock("GET", "/health")
        .with_status(200)
        .with_body(r#"{"status":"healthy"}"#)
        .create();

    let client = ConfigClient::new(server.url()).unwrap();
    let healthy = client.health_check().await.unwrap();
    assert!(healthy);
}

#[tokio::test]
async fn test_get_config() {
    let mut server = mockito::Server::new_async().await;

    let response_body = r#"{
        "application": "myapp",
        "environment": "dev",
        "config_name": "database",
        "version": "v1",
        "content": {"host": "localhost", "port": 5432},
        "schema": {"type": "object"}
    }"#;

    let _m = server
        .mock("GET", "/configs/myapp/dev/database")
        .with_status(200)
        .with_body(response_body)
        .create();

    let client = ConfigClient::new(server.url()).unwrap();
    let key = ConfigKey::new("myapp", "dev", "database");
    let config = client.get_config(&key).await.unwrap();

    assert_eq!(config.version, "v1");
    assert_eq!(config.content["host"], "localhost");
    assert_eq!(config.content["port"], 5432);
}

#[tokio::test]
async fn test_get_config_not_found() {
    let mut server = mockito::Server::new_async().await;

    let _m = server
        .mock("GET", "/configs/myapp/dev/missing")
        .with_status(404)
        .create();

    let client = ConfigClient::new(server.url()).unwrap();
    let key = ConfigKey::new("myapp", "dev", "missing");
    let result = client.get_config(&key).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_put_config() {
    let mut server = mockito::Server::new_async().await;

    let _m = server
        .mock("PUT", "/configs/myapp/dev/api")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Json(json!({
            "content": {"url": "https://api.example.com"},
            "schema": {"type": "object"},
            "expected_version": null
        })))
        .with_status(200)
        .with_body(r#"{"message": "Success", "version": "v1"}"#)
        .create();

    let client = ConfigClient::new(server.url()).unwrap();
    let key = ConfigKey::new("myapp", "dev", "api");
    let content = json!({"url": "https://api.example.com"});
    let schema = json!({"type": "object"});

    let version = client
        .put_config(&key, content, Some(schema), None)
        .await
        .unwrap();
    assert_eq!(version, "v1");
}

#[tokio::test]
async fn test_delete_config() {
    let mut server = mockito::Server::new_async().await;

    let _m = server
        .mock("DELETE", "/configs/myapp/dev/temp")
        .with_status(200)
        .with_body(r#"{"message": "Deleted successfully"}"#)
        .create();

    let client = ConfigClient::new(server.url()).unwrap();
    let key = ConfigKey::new("myapp", "dev", "temp");

    client.delete_config(&key).await.unwrap();
}

#[tokio::test]
async fn test_list_versions() {
    let mut server = mockito::Server::new_async().await;

    let response_body = r#"{
        "versions": [
            {"version": "v1", "timestamp": "2024-01-01T00:00:00Z"},
            {"version": "v2", "timestamp": "2024-01-02T00:00:00Z"}
        ]
    }"#;

    let _m = server
        .mock("GET", "/configs/myapp/dev/config/versions")
        .with_status(200)
        .with_body(response_body)
        .create();

    let client = ConfigClient::new(server.url()).unwrap();
    let key = ConfigKey::new("myapp", "dev", "config");
    let versions = client.list_versions(&key).await.unwrap();

    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].version, "v1");
    assert_eq!(versions[1].version, "v2");
}

#[tokio::test]
async fn test_list_configs() {
    let mut server = mockito::Server::new_async().await;

    let response_body = r#"{
        "configs": [
            {"application": "app1", "environment": "dev", "config_name": "db"},
            {"application": "app2", "environment": "prod", "config_name": "api"}
        ]
    }"#;

    let _m = server
        .mock("GET", "/configs")
        .with_status(200)
        .with_body(response_body)
        .create();

    let client = ConfigClient::new(server.url()).unwrap();
    let configs = client.list_configs(None).await.unwrap();

    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].application, "app1");
    assert_eq!(configs[1].application, "app2");
}

#[tokio::test]
async fn test_list_configs_with_prefix() {
    let mut server = mockito::Server::new_async().await;

    let _m = server
        .mock("GET", "/configs?prefix=app1")
        .with_status(200)
        .with_body(r#"{"configs": []}"#)
        .create();

    let client = ConfigClient::new(server.url()).unwrap();
    let configs = client.list_configs(Some("app1")).await.unwrap();

    assert_eq!(configs.len(), 0);
}
