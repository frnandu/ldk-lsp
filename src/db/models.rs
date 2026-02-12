//! Database models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

/// JIT Receive request database model
/// For users lacking inbound liquidity who want to receive payments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiveRequestModel {
    /// Request ID (UUID)
    pub id: String,
    /// Client node ID (who wants to receive)
    pub node_id: String,
    /// Client host address
    pub host: String,
    /// Client port
    pub port: i32,
    /// Amount user wants to receive (satoshis)
    pub amount: i64,
    /// Base LSP fee (satoshis)
    pub fee_base: i64,
    /// PPM fee (satoshis)
    pub fee_ppm: i64,
    /// Estimated onchain fee (satoshis)
    pub fee_onchain: i64,
    /// Total fee (base + ppm + onchain)
    pub fee_total: i64,
    /// Fee rate used for onchain calculation (sat/vbyte)
    pub fee_rate: i64,
    /// Total amount on invoice (amount + fee_total)
    pub total_invoice_amount: i64,
    /// Total channel capacity (amount + inbound_buffer)
    pub total_channel_capacity: i64,
    /// Whether this will be a splice (true) or new channel (false)
    pub is_splice: bool,
    /// Channel ID (if splice, the existing channel; if new channel, the new channel ID after opening)
    pub channel_id: Option<String>,
    /// Request status
    pub status: String,
    /// Payment hash for invoice
    pub payment_hash: Option<String>,
    /// Failure reason (if failed)
    pub failure_reason: Option<String>,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Last update time
    pub updated_at: DateTime<Utc>,
}
