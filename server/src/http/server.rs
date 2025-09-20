use anyhow::Result;
use axum::{routing::get, Router};
use std::{net::SocketAddr, sync::Arc};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use super::{handlers, state::AppState};
use crate::storage::ConfigStorage;

pub async fn start_server(storage: Arc<dyn ConfigStorage>, bind_address: SocketAddr) -> Result<()> {
    let app_state = Arc::new(AppState { storage });

    // Build the router
    let app = Router::new()
        // Health check
        .route("/health", get(handlers::health_check))
        // Config CRUD operations
        .route("/configs", get(handlers::list_configs))
        .route(
            "/configs/:app/:env/:config",
            get(handlers::get_config)
                .put(handlers::put_config)
                .delete(handlers::delete_config),
        )
        // Version operations
        .route(
            "/configs/:app/:env/:config/versions",
            get(handlers::list_versions),
        )
        .route(
            "/configs/:app/:env/:config/versions/:version",
            get(handlers::get_config_version),
        )
        // Add state
        .with_state(app_state)
        // Add middleware
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    info!("Server listening on {}", bind_address);

    // Run the server
    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
