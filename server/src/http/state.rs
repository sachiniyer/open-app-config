use crate::storage::ConfigStorage;
use std::sync::Arc;

/// Application state shared across handlers
#[derive(Clone)]
#[allow(dead_code)]
pub struct AppState {
    pub storage: Arc<dyn ConfigStorage>,
}