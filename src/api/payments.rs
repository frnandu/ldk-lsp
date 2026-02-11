//! Payment webhook handlers

use super::{ApiResponse, ApiState};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Payment webhook payload
#[derive(Debug, Deserialize)]
pub struct PaymentWebhookRequest {
    /// Payment hash
    pub payment_hash: String,
    /// Payment amount in millisatoshis
    pub amount_msat: u64,
    /// Payment status
    pub status: String,
    /// Associated request ID (if known)
    pub request_id: Option<String>,
    /// Payment preimage (if successful)
    pub preimage: Option<String>,
}

/// Payment webhook response
#[derive(Debug, Serialize)]
pub struct PaymentWebhookResponse {
    /// Whether the webhook was processed successfully
    pub processed: bool,
    /// Message
    pub message: String,
}

/// Handle payment webhook
///
/// This endpoint is called by the payment processor when a payment is received.
/// It triggers the channel opening process for pending channel requests.
pub async fn handle_payment_webhook(
    State(state): State<ApiState>,
    Json(req): Json<PaymentWebhookRequest>,
) -> impl IntoResponse {
    info!(
        "Received payment webhook: hash={}, amount={} msat, status={}",
        req.payment_hash, req.amount_msat, req.status
    );

    // TODO: Verify webhook signature/authentication

    // TODO: Process payment and trigger channel opening if applicable

    let response = PaymentWebhookResponse {
        processed: true,
        message: "Payment received".to_string(),
    };

    (StatusCode::OK, Json(ApiResponse::success(response)))
}