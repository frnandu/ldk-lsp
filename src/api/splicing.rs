//! Splicing API endpoints

use super::{error_to_status_code, ApiResponse, ApiState};
use crate::lsp::SpliceQuote;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Splice quote request
#[derive(Debug, Deserialize)]
pub struct SpliceQuoteRequest {
    /// Channel ID to splice
    pub channel_id: String,
    /// Additional capacity to add (splice in)
    pub additional_capacity: u64,
}

/// Splice quote response
#[derive(Debug, Serialize)]
pub struct SpliceQuoteResponse {
    /// Splice request ID
    pub splice_id: String,
    /// Channel ID
    pub channel_id: String,
    /// Additional capacity
    pub additional_capacity: u64,
    /// LSP fee
    pub fee: u64,
    /// Total cost
    pub total_cost: u64,
    /// Quote expiry
    pub expiry: String,
    /// Payment invoice
    pub invoice: String,
    /// Payment hash
    pub payment_hash: String,
}

/// Request a splice quote
pub async fn request_splice_quote(
    State(state): State<ApiState>,
    Json(req): Json<SpliceQuoteRequest>,
) -> impl IntoResponse {
    info!(
        "API: Splice quote request for channel_id={}, additional_capacity={} sats",
        req.channel_id, req.additional_capacity
    );

    let lsp_service = crate::lsp::LspService::new(state.app.config.clone(), state.app.db.clone());

    let channel_id = crate::node::ChannelId(req.channel_id);

    match lsp_service
        .request_splice(&channel_id, req.additional_capacity, state.app.node.clone())
        .await
    {
        Ok((quote, invoice)) => {
            info!(
                "API: Splice quote created: splice_id={}, additional_capacity={}, fee={}",
                quote.splice_id, quote.additional_capacity, quote.fee
            );
            let response = SpliceQuoteResponse {
                splice_id: quote.splice_id,
                channel_id: quote.channel_id.to_string(),
                additional_capacity: quote.additional_capacity,
                fee: quote.fee,
                total_cost: quote.total_cost,
                expiry: quote.expiry.to_rfc3339(),
                invoice,
                payment_hash: quote.payment_hash,
            };
            (StatusCode::OK, Json(ApiResponse::success(response)))
        }
        Err(e) => {
            let status = error_to_status_code(&e);
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Confirm a splice request
pub async fn confirm_splice(
    State(state): State<ApiState>,
    Path(splice_id): Path<String>,
) -> impl IntoResponse {
    info!("API: Confirm splice request: splice_id={}", splice_id);

    // TODO: Implement splice confirmation
    let response = serde_json::json!({
        "splice_id": splice_id,
        "status": "initiated",
    });
    (StatusCode::OK, Json(ApiResponse::success(response)))
}

/// Get splice request status
#[derive(Debug, Serialize)]
pub struct SpliceRequestResponse {
    /// Splice ID
    pub splice_id: String,
    /// Channel ID
    pub channel_id: String,
    /// Status
    pub status: String,
    /// Additional capacity
    pub additional_capacity: u64,
    /// Fee
    pub fee: u64,
}

/// Get a splice request
pub async fn get_splice_request(
    State(state): State<ApiState>,
    Path(splice_id): Path<String>,
) -> impl IntoResponse {
    info!("API: Get splice request: splice_id={}", splice_id);

    let lsp_service = crate::lsp::LspService::new(state.app.config.clone(), state.app.db.clone());

    match lsp_service.get_splice_request(&splice_id).await {
        Some(req) => {
            let response = SpliceRequestResponse {
                splice_id: req.splice_id,
                channel_id: req.channel_id.to_string(),
                status: format!("{:?}", req.status),
                additional_capacity: match req.operation {
                    crate::lsp::SpliceType::SpliceIn { additional_capacity } => additional_capacity,
                    _ => 0,
                },
                fee: 0, // TODO: Calculate fee
            };
            (StatusCode::OK, Json(ApiResponse::success(response)))
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Splice request not found")),
        ),
    }
}