//! Channel management API endpoints

use super::{error_to_status_code, ApiResponse, ApiState, PaginationParams, PaginatedResponse};
use crate::lsp::{ChannelQuote, ChannelRequest, ChannelRequestStatus};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Request a channel quote
#[derive(Debug, Deserialize)]
pub struct ChannelQuoteRequest {
    /// Client node ID (public key)
    pub node_id: String,
    /// Client host address
    pub host: String,
    /// Client port
    pub port: u16,
    /// Desired channel capacity (satoshis)
    pub capacity: u64,
    /// Whether to enable zero-confirmation
    #[serde(default)]
    pub require_zeroconf: bool,
}

/// Channel quote response
#[derive(Debug, Serialize)]
pub struct ChannelQuoteResponse {
    /// Request ID
    pub request_id: String,
    /// Channel capacity
    pub capacity: u64,
    /// LSP fee
    pub fee: u64,
    /// Total cost (capacity + fee)
    pub total_cost: u64,
    /// LSP node ID
    pub lsp_node_id: String,
    /// Quote expiry timestamp (ISO 8601)
    pub expiry: String,
    /// Payment invoice (if payment is required upfront)
    pub payment_invoice: Option<String>,
}

/// Request a channel quote
pub async fn request_channel_quote(
    State(state): State<ApiState>,
    Json(req): Json<ChannelQuoteRequest>,
) -> impl IntoResponse {
    info!(
        "API: Channel quote request from node_id={} ({}:{}), capacity={} sats, zeroconf={}",
        req.node_id, req.host, req.port, req.capacity, req.require_zeroconf
    );

    // Create the LSP service with database
    let lsp_service = crate::lsp::LspService::new(state.app.config.clone(), state.app.db.clone());

    // Get the channel quote and invoice (this saves to database with payment hash)
    match lsp_service
        .request_channel(
            &req.node_id,
            &req.host,
            req.port,
            req.capacity,
            req.require_zeroconf,
            state.app.node.clone(),
        )
        .await
    {
        Ok((quote, invoice)) => {
            info!(
                "API: Channel quote created: request_id={}, capacity={}, fee={}, total={}",
                quote.request_id, quote.capacity, quote.fee, quote.total_cost
            );
            
            let response = ChannelQuoteResponse {
                request_id: quote.request_id,
                capacity: quote.capacity,
                fee: quote.fee,
                total_cost: quote.total_cost,
                lsp_node_id: quote.lsp_node_id,
                expiry: quote.expiry.to_rfc3339(),
                payment_invoice: Some(invoice),
            };
            (StatusCode::OK, Json(ApiResponse::success(response)))
        }
        Err(e) => {
            let status = error_to_status_code(&e);
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Confirm a channel request (after payment received)
pub async fn confirm_channel(
    State(state): State<ApiState>,
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    info!("API: Confirm channel request: request_id={}", request_id);

    let lsp_service = crate::lsp::LspService::new(state.app.config.clone(), state.app.db.clone());

    // Check status - this will detect payment and open channel if paid
    match lsp_service
        .check_request_status(&request_id, state.app.node.clone())
        .await
    {
        Ok(status) => {
            match status {
                ChannelRequestStatus::ChannelOpened(channel_id) => {
                    let response = serde_json::json!({
                        "channel_id": channel_id.to_string(),
                        "status": "opened",
                    });
                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                ChannelRequestStatus::OpeningChannel => {
                    let response = serde_json::json!({
                        "status": "opening",
                    });
                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                ChannelRequestStatus::PendingPayment => {
                    let response = serde_json::json!({
                        "status": "pending_payment",
                    });
                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                ChannelRequestStatus::Expired => {
                    let response = serde_json::json!({
                        "status": "expired",
                    });
                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                _ => {
                    let response = serde_json::json!({
                        "status": format!("{:?}", status),
                    });
                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
            }
        }
        Err(e) => {
            let status = error_to_status_code(&e);
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Get channel request status
#[derive(Debug, Serialize)]
pub struct ChannelRequestResponse {
    /// Request ID
    pub request_id: String,
    /// Current status
    pub status: String,
    /// Channel ID (if opened)
    pub channel_id: Option<String>,
    /// Request details
    pub node_id: String,
    pub capacity: u64,
    pub fee: u64,
    pub require_zeroconf: bool,
    /// Creation time
    pub created_at: String,
}

/// Get a channel request
pub async fn get_channel_request(
    State(state): State<ApiState>,
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    info!("API: Get channel request: request_id={}", request_id);

    let lsp_service = crate::lsp::LspService::new(state.app.config.clone(), state.app.db.clone());

    // First check status - this will verify payment and open channel if needed
    let status_result = lsp_service
        .check_request_status(&request_id, state.app.node.clone())
        .await;

    match status_result {
        Ok(current_status) => {
            // Get updated request from database
            match lsp_service.get_request(&request_id).await {
                Ok(Some(req)) => {
                    let response = ChannelRequestResponse {
                        request_id: req.id,
                        status: format!("{:?}", current_status),
                        channel_id: match current_status {
                            ChannelRequestStatus::ChannelOpened(id) => Some(id.to_string()),
                            _ => None,
                        },
                        node_id: req.node_id,
                        capacity: req.capacity,
                        fee: req.fee,
                        require_zeroconf: req.require_zeroconf,
                        created_at: req.created_at.to_rfc3339(),
                    };
                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                Ok(None) => (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::error("Channel request not found")),
                ),
                Err(e) => {
                    let status = error_to_status_code(&e);
                    (status, Json(ApiResponse::error(e.to_string())))
                }
            }
        }
        Err(e) => {
            let status = error_to_status_code(&e);
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// List all channels
#[derive(Debug, Serialize)]
pub struct ChannelInfoResponse {
    /// Channel ID
    pub channel_id: String,
    /// Counterparty node ID
    pub counterparty: String,
    /// Channel capacity
    pub capacity: u64,
    /// Local balance
    pub local_balance: u64,
    /// Remote balance
    pub remote_balance: u64,
    /// Is channel ready
    pub is_ready: bool,
    /// Is public channel
    pub is_public: bool,
}

/// List channels
pub async fn list_channels(
    State(state): State<ApiState>,
    Query(params): Query<PaginationParams>,
) -> impl IntoResponse {
    info!("API: List channels request (page={}, per_page={})", params.page, params.per_page);

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

            let total = items.len() as u64;
            let response = PaginatedResponse::new(items, total, params.page, params.per_page);

            (StatusCode::OK, Json(ApiResponse::success(response)))
        }
        Err(e) => {
            let status = error_to_status_code(&e);
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Get node info
#[derive(Debug, Serialize)]
pub struct NodeInfoResponse {
    /// Node ID
    pub node_id: String,
    /// Network
    pub network: String,
    /// Block height
    pub block_height: u32,
    /// Number of peers
    pub num_peers: usize,
    /// Number of channels
    pub num_channels: usize,
}

/// Get node information
pub async fn get_node_info(State(state): State<ApiState>) -> impl IntoResponse {
    info!("API: Get node info request");

    let node = state.app.node.read().await;

    match node.get_node_info().await {
        Ok(info) => {
            let response = NodeInfoResponse {
                node_id: info.node_id,
                network: info.network,
                block_height: info.block_height,
                num_peers: info.num_peers,
                num_channels: info.num_channels,
            };
            (StatusCode::OK, Json(ApiResponse::success(response)))
        }
        Err(e) => {
            let status = error_to_status_code(&e);
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}
