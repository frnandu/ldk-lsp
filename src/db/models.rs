//! Database models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Channel request database model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelRequestModel {
    /// Request ID
    pub id: String,
    /// Client node ID
    pub node_id: String,
    /// Client host
    pub host: String,
    /// Client port
    pub port: i32,
    /// Requested capacity
    pub capacity: i64,
    /// LSP fee
    pub fee: i64,
    /// Whether zero-confirmation is requested
    pub require_zeroconf: bool,
    /// Request status
    pub status: String,
    /// Channel ID (if opened)
    pub channel_id: Option<String>,
    /// Payment hash
    pub payment_hash: Option<String>,
    /// Failure/closure reason (if failed or closed)
    pub failure_reason: Option<String>,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Last update time
    pub updated_at: DateTime<Utc>,
}

/// Payment database model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentModel {
    /// Payment ID
    pub id: String,
    /// Payment hash
    pub payment_hash: String,
    /// Associated request ID
    pub request_id: Option<String>,
    /// Amount in millisatoshis
    pub amount_msat: i64,
    /// Payment status
    pub status: String,
    /// Payment preimage
    pub preimage: Option<String>,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Settlement time
    pub settled_at: Option<DateTime<Utc>>,
}

/// Splice request database model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpliceRequestModel {
    /// Splice ID
    pub id: String,
    /// Channel ID
    pub channel_id: String,
    /// Operation type (splice_in or splice_out)
    pub operation_type: String,
    /// Amount
    pub amount: i64,
    /// Bitcoin address (for splice_out)
    pub address: Option<String>,
    /// Status
    pub status: String,
    /// Transaction ID
    pub txid: Option<String>,
    /// Payment hash for invoice
    pub payment_hash: Option<String>,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Last update time
    pub updated_at: DateTime<Utc>,
}

/// Peer database model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerModel {
    /// Node ID
    pub node_id: String,
    /// Node alias
    pub alias: Option<String>,
    /// First seen time
    pub first_seen: DateTime<Utc>,
    /// Last connected time
    pub last_connected: Option<DateTime<Utc>>,
    /// Trust score
    pub trust_score: String,
    /// Total number of channels opened
    pub total_channels: i32,
    /// Number of zeroconf violations
    pub zeroconf_violations: i32,
}
