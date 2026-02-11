//! Health check endpoints

use super::{ApiResponse, ApiState};
use axum::{extract::State, response::Json};
use serde::Serialize;

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Service version
    pub version: String,
    /// LDK Server connection status
    pub ldk_server_connected: bool,
}

/// Health check endpoint
pub async fn health_check(State(state): State<ApiState>) -> Json<ApiResponse<HealthResponse>> {
    // Check LDK Server connection
    let ldk_connected = {
        let node = state.app.node.read().await;
        node.client().is_ok()
    };

    let response = HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        ldk_server_connected: ldk_connected,
    };

    Json(ApiResponse::success(response))
}