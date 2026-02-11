//! HTTP API for LSP services
//!
//! This module provides a RESTful API for:
//! - Requesting channel opens
//! - Managing splicing operations
//! - Querying channel status
//! - Webhook callbacks for payment notifications

use crate::{config::Config, LspApp, LspResult};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{debug, error, info, warn};

mod channels;
mod health;
mod payments;
mod splicing;

pub use channels::*;
pub use health::*;
pub use payments::*;
pub use splicing::*;

/// API state shared across handlers
#[derive(Clone)]
pub struct ApiState {
    /// The LSP application
    pub app: LspApp,
}

/// Build the API router
fn build_router(app: LspApp) -> Router {
    let state = ApiState { app };

    Router::new()
        // Health check
        .route("/health", get(health_check))
        // Channel endpoints
        .route("/v1/channels/quote", post(request_channel_quote))
        .route("/v1/channels/:request_id/confirm", post(confirm_channel))
        .route("/v1/channels/:request_id", get(get_channel_request))
        // Splicing endpoints
        .route("/v1/splice/quote", post(request_splice_quote))
        .route("/v1/splice/:splice_id/confirm", post(confirm_splice))
        .route("/v1/splice/:splice_id", get(get_splice_request))
        // Payment webhook
        .route("/v1/webhook/payment", post(handle_payment_webhook))
        // Node info
        .route("/v1/node/info", get(get_node_info))
        .route("/v1/node/channels", get(list_channels))
        // Add state
        .with_state(state)
}

/// Start the HTTP API server
pub async fn serve(app: LspApp) -> anyhow::Result<()> {
    serve_with_shutdown(app, tokio::sync::oneshot::channel().1).await
}

/// Start the HTTP API server with graceful shutdown
pub async fn serve_with_shutdown(
    app: LspApp,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let config = app.config.clone();
    
    // Build the router
    let router = build_router(app);

    // Add CORS if enabled
    let router = if config.api.enable_cors {
        router.layer(CorsLayer::permissive())
    } else {
        router
    };

    // Parse bind address
    let addr: std::net::SocketAddr = config
        .api
        .bind_address
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid bind address: {}", e))?;

    info!("Starting HTTP API server on {}", addr);

    // Start the server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
            info!("Received shutdown signal, stopping API server...");
        })
        .await?;

    info!("API server stopped gracefully");
    Ok(())
}

/// Standard API response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Whether the request was successful
    pub success: bool,
    /// Response data (only present if success is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Error message (only present if success is false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    /// Create a successful response
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// API error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
}

/// Convert LspError to HTTP status code
pub fn error_to_status_code(err: &crate::LspError) -> StatusCode {
    use crate::LspError;
    match err {
        LspError::Validation(_) => StatusCode::BAD_REQUEST,
        LspError::Channel(_) => StatusCode::CONFLICT,
        LspError::Node(_) => StatusCode::SERVICE_UNAVAILABLE,
        LspError::Grpc(_) => StatusCode::BAD_GATEWAY,
        LspError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        LspError::Api(_) => StatusCode::INTERNAL_SERVER_ERROR,
        LspError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Pagination parameters
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    /// Page number (1-based)
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 {
    1
}

fn default_per_page() -> u32 {
    20
}

/// Paginated response
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    /// Items for this page
    pub items: Vec<T>,
    /// Total number of items
    pub total: u64,
    /// Current page
    pub page: u32,
    /// Items per page
    pub per_page: u32,
    /// Total pages
    pub total_pages: u32,
}

impl<T> PaginatedResponse<T> {
    /// Create a paginated response
    pub fn new(items: Vec<T>, total: u64, page: u32, per_page: u32) -> Self {
        let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;
        Self {
            items,
            total,
            page,
            per_page,
            total_pages,
        }
    }
}