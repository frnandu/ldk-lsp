//! Channel management API endpoints (DEPRECATED)
//! 
//! These endpoints are no longer used. Use /v1/receive endpoints instead.

use super::{ApiResponse, ApiState, PaginationParams};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};

/// Request a channel quote (DEPRECATED)
#[derive(Debug, Deserialize)]
pub struct ChannelQuoteRequest {
    pub node_id: String,
    pub host: String,
    pub port: u16,
    pub capacity: u64,
    #[serde(default)]
    pub require_zeroconf: bool,
}

/// Channel quote response
#[derive(Debug, Serialize)]
pub struct ChannelQuoteResponse {
    pub request_id: String,
    pub capacity: u64,
    pub fee: u64,
    pub total_cost: u64,
    pub lsp_node_id: String,
    pub expiry: String,
    pub payment_invoice: Option<String>,
}

/// Request a channel quote (DEPRECATED - use /v1/receive/quote instead)
pub async fn request_channel_quote(
    _state: State<ApiState>,
    _req: Json<ChannelQuoteRequest>,
) -> impl IntoResponse {
    let response: ApiResponse<ChannelQuoteResponse> = ApiResponse::error("This endpoint is deprecated. Use POST /v1/receive/quote for JIT liquidity requests.");
    (
        StatusCode::GONE,
        Json(response),
    )
}

/// Confirm a channel request (DEPRECATED)
pub async fn confirm_channel(
    _state: State<ApiState>,
    Path(_request_id): Path<String>,
) -> impl IntoResponse {
    let response: ApiResponse<serde_json::Value> = ApiResponse::error("This endpoint is deprecated. Payment confirmation is now automatic via RabbitMQ events.");
    (
        StatusCode::GONE,
        Json(response),
    )
}

/// Channel request response (DEPRECATED)
#[derive(Debug, Serialize)]
pub struct ChannelRequestResponse {
    pub request_id: String,
    pub status: String,
    pub channel_id: Option<String>,
    pub node_id: String,
    pub capacity: u64,
    pub fee: u64,
    pub require_zeroconf: bool,
    pub created_at: String,
}

/// Get a channel request (DEPRECATED)
pub async fn get_channel_request(
    _state: State<ApiState>,
    Path(_request_id): Path<String>,
) -> impl IntoResponse {
    let response: ApiResponse<ChannelRequestResponse> = ApiResponse::error("This endpoint is deprecated. Use GET /v1/receive/:receive_id instead.");
    (
        StatusCode::GONE,
        Json(response),
    )
}

/// Channel info response
#[derive(Debug, Serialize)]
pub struct ChannelInfoResponse {
    pub channel_id: String,
    pub counterparty: String,
    pub capacity: u64,
    pub local_balance: u64,
    pub remote_balance: u64,
    pub is_ready: bool,
    pub is_public: bool,
}

/// Node info response
#[derive(Debug, Serialize)]
pub struct NodeInfoResponse {
    pub node_id: String,
    pub network: String,
}

/// Get node info
pub async fn get_node_info(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let node = state.app.node.read().await;
    
    match node.get_node_info().await {
        Ok(info) => {
            let response = NodeInfoResponse {
                node_id: info.node_id,
                network: info.network,
            };
            (StatusCode::OK, Json(ApiResponse::success(response)))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

/// List channels (still functional - returns actual LDK channels)
pub async fn list_channels(
    State(state): State<ApiState>,
    Query(_params): Query<PaginationParams>,
) -> impl IntoResponse {
    let node = state.app.node.read().await;

    match node.list_channels().await {
        Ok(channels) => {
            let items: Vec<ChannelInfoResponse> = channels
                .into_iter()
                .map(|c| ChannelInfoResponse {
                    channel_id: c.id.to_string(),
                    counterparty: c.counterparty_node_id,
                    capacity: c.capacity_sat,
                    local_balance: c.local_balance_sat,
                    remote_balance: c.remote_balance_sat,
                    is_ready: c.is_ready,
                    is_public: c.is_public,
                })
                .collect();
            
            (StatusCode::OK, Json(ApiResponse::success(items)))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}
