use crate::storage::ConfigStorage;
use std::sync::Arc;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn ConfigStorage>,
}
