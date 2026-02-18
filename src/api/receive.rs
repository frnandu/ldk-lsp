//! Receive API endpoints for JIT liquidity
//!
//! This module provides endpoints for users to request inbound liquidity
//! so they can receive Lightning payments even without existing channels.

use super::{error_to_status_code, ApiResponse, ApiState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Request a receive quote
#[derive(Debug, Deserialize)]
pub struct ReceiveQuoteRequest {
    /// Client node ID (public key)
    pub node_id: String,
    /// Client host address
    pub host: String,
    /// Client port
    pub port: u16,
    /// Amount the user wants to receive (satoshis)
    pub amount: u64,
    /// BOLT11 invoice for the amount the user wants to receive
    /// After successful splice/channel open, LSP will pay this invoice
    pub invoice: String,
}

/// Fee breakdown in the response
#[derive(Debug, Serialize)]
pub struct FeeBreakdown {
    /// Base LSP fee (satoshis)
    pub base: u64,
    /// PPM fee based on amount (satoshis)
    pub ppm: u64,
    /// Estimated onchain fee (satoshis)
    pub onchain: u64,
    /// Channel reserve amount (satoshis)
    /// This ensures the user receives their full requested amount after the reserve is locked
    pub reserve: u64,
    /// Total fee (base + ppm + onchain + reserve)
    pub total: u64,
    /// Fee rate used for onchain calculation (sat/vbyte)
    pub fee_rate: u64,
}

/// Receive quote response
#[derive(Debug, Serialize)]
pub struct ReceiveQuoteResponse {
    /// Request ID
    pub receive_id: String,
    /// Amount user wants to receive
    pub amount: u64,
    /// Fee breakdown
    pub fees: FeeBreakdown,
    /// Total amount to pay (amount + fees.total)
    pub total_invoice_amount: u64,
    /// Whether this will be a splice (true) or new channel (false)
    pub is_splice: bool,
    /// Channel ID if splicing (existing channel)
    pub channel_id: Option<String>,
    /// LSP node ID
    pub lsp_node_id: String,
    /// Quote expiry timestamp (ISO 8601)
    pub expiry: String,
    /// Payment invoice
    pub invoice: String,
    /// Payment hash
    pub payment_hash: String,
}

/// Request a receive quote
pub async fn request_receive_quote(
    State(state): State<ApiState>,
    Json(req): Json<ReceiveQuoteRequest>,
) -> impl IntoResponse {
    info!(
        "API: Receive quote request from node_id={} ({}:{}), amount={} sats",
        req.node_id, req.host, req.port, req.amount
    );

    // Create the LSP service with database
    let lsp_service = crate::lsp::LspService::new(state.app.config.clone(), state.app.db.clone());

    // Get the receive quote and invoice
    match lsp_service
        .request_receive_quote(
            &req.node_id,
            &req.host,
            req.port,
            req.amount,
            &req.invoice,
            state.app.node.clone(),
        )
        .await
    {
        Ok((quote, invoice)) => {
            info!(
                "API: Receive quote created: receive_id={}, amount={}, total={}",
                quote.receive_id, quote.amount, quote.total_invoice_amount
            );

            let response = ReceiveQuoteResponse {
                receive_id: quote.receive_id,
                amount: quote.amount,
                fees: FeeBreakdown {
                    base: quote.fee_base,
                    ppm: quote.fee_ppm,
                    onchain: quote.fee_onchain,
                    reserve: quote.reserve_amount,
                    total: quote.fee_total,
                    fee_rate: quote.fee_rate,
                },
                total_invoice_amount: quote.total_invoice_amount,
                is_splice: quote.is_splice,
                channel_id: quote.channel_id.map(|c| c.to_string()),
                lsp_node_id: quote.lsp_node_id,
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

/// Receive request status response
#[derive(Debug, Serialize)]
pub struct ReceiveRequestResponse {
    /// Request ID
    pub receive_id: String,
    /// Current status
    pub status: String,
    /// Amount user wants to receive
    pub amount: u64,
    /// Whether this is a splice
    pub is_splice: bool,
    /// Channel ID (if opened/spliced)
    pub channel_id: Option<String>,
    /// Fee breakdown
    pub fees: FeeBreakdown,
    /// Total invoice amount
    pub total_invoice_amount: u64,
    /// Creation time
    pub created_at: String,
    /// Failure reason (if failed)
    pub failure_reason: Option<String>,
}

/// Get a receive request status
pub async fn get_receive_request(
    State(state): State<ApiState>,
    Path(receive_id): Path<String>,
) -> impl IntoResponse {
    info!("API: Get receive request: receive_id={}", receive_id);

    let lsp_service = crate::lsp::LspService::new(state.app.config.clone(), state.app.db.clone());

    // Check status - this will verify payment and trigger action if needed
    let status_result = lsp_service
        .check_receive_status(&receive_id, state.app.node.clone())
        .await;

    match status_result {
        Ok(current_status) => {
            // Get updated request from database
            match lsp_service.get_receive_request(&receive_id).await {
                Ok(Some(req)) => {
                    let response = ReceiveRequestResponse {
                        receive_id: req.id,
                        status: current_status.as_str().to_string(),
                        amount: req.amount,
                        is_splice: req.is_splice,
                        channel_id: match current_status {
                            crate::lsp::ReceiveRequestStatus::ChannelOpened(id) |
                            crate::lsp::ReceiveRequestStatus::SpliceCompleted(id) => Some(id.to_string()),
                            _ => req.channel_id.map(|c| c.to_string()),
                        },
                        fees: FeeBreakdown {
                            base: req.fee_base,
                            ppm: req.fee_ppm,
                            onchain: req.fee_onchain,
                            reserve: 0, // Reserve not stored in DB for existing requests
                            total: req.fee_total,
                            fee_rate: req.fee_rate,
                        },
                        total_invoice_amount: req.total_invoice_amount,
                        created_at: req.created_at.to_rfc3339(),
                        failure_reason: req.failure_reason,
                    };
                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                Ok(None) => (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::error("Receive request not found")),
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
