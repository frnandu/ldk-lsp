//! Splicing API endpoints (DEPRECATED)
//!
//! These endpoints are no longer used. Splicing is now handled automatically
//! through the JIT receive flow via /v1/receive/quote.

use super::{ApiResponse, ApiState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};

/// Splice quote request (DEPRECATED)
#[derive(Debug, Deserialize)]
pub struct SpliceQuoteRequest {
    pub channel_id: String,
    pub additional_capacity: u64,
}

/// Splice quote response
#[derive(Debug, Serialize)]
pub struct SpliceQuoteResponse {
    pub splice_id: String,
    pub channel_id: String,
    pub additional_capacity: u64,
    pub fee: u64,
    pub total_cost: u64,
    pub expiry: String,
    pub invoice: String,
    pub payment_hash: String,
}

/// Request a splice quote (DEPRECATED - use /v1/receive/quote instead)
pub async fn request_splice_quote(
    _state: State<ApiState>,
    _req: Json<SpliceQuoteRequest>,
) -> impl IntoResponse {
    let response: ApiResponse<SpliceQuoteResponse> = ApiResponse::error("This endpoint is deprecated. Use POST /v1/receive/quote instead. Splicing is handled automatically based on existing channels.");
    (
        StatusCode::GONE,
        Json(response),
    )
}

/// Confirm a splice (DEPRECATED)
pub async fn confirm_splice(
    _state: State<ApiState>,
    Path(_splice_id): Path<String>,
) -> impl IntoResponse {
    let response: ApiResponse<serde_json::Value> = ApiResponse::error("This endpoint is deprecated. Payment confirmation is now automatic via RabbitMQ events.");
    (
        StatusCode::GONE,
        Json(response),
    )
}

/// Splice request response (DEPRECATED)
#[derive(Debug, Serialize)]
pub struct SpliceRequestResponse {
    pub splice_id: String,
    pub channel_id: String,
    pub additional_capacity: u64,
    pub fee: u64,
    pub status: String,
    pub created_at: String,
}

/// Get a splice request (DEPRECATED)
pub async fn get_splice_request(
    _state: State<ApiState>,
    Path(_splice_id): Path<String>,
) -> impl IntoResponse {
    let response: ApiResponse<SpliceRequestResponse> = ApiResponse::error("This endpoint is deprecated.");
    (
        StatusCode::GONE,
        Json(response),
    )
}
