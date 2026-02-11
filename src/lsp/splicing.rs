//! Channel splicing service
//!
//! Splicing allows increasing or decreasing a channel's capacity
//! without closing and reopening the channel, maintaining the
//! channel's history and HTLCs.

use crate::{
    config::Config,
    node::{ChannelId, LspNode},
    LspResult,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Service for managing channel splicing
pub struct SplicingService {
    /// Configuration
    config: Arc<Config>,
    /// Pending splice operations
    pending_splices: Arc<RwLock<Vec<SpliceRequest>>>,
}

impl SplicingService {
    /// Create a new splicing service
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            pending_splices: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Initialize the splicing service
    pub async fn init(&self, _node: Arc<RwLock<LspNode>>) -> LspResult<()> {
        info!("Initializing splicing service...");

        if !self.config.lsp.enable_splicing {
            info!("Channel splicing is disabled");
            return Ok(());
        }

        info!("Splicing service initialized");
        Ok(())
    }

    /// Request a splice-in operation (increase capacity)
    pub async fn request_splice_in(
        &self,
        channel_id: ChannelId,
        additional_capacity: u64,
    ) -> LspResult<SpliceRequest> {
        if !self.config.lsp.enable_splicing {
            return Err(crate::LspError::Validation(
                "Channel splicing is not enabled".to_string(),
            ));
        }

        debug!(
            "Requesting splice-in for channel {} with {} additional sats",
            channel_id, additional_capacity
        );

        let splice_id = uuid::Uuid::new_v4().to_string();

        let request = SpliceRequest {
            splice_id: splice_id.clone(),
            channel_id,
            operation: SpliceType::SpliceIn { additional_capacity },
            status: SpliceStatus::PendingPayment,
            created_at: chrono::Utc::now(),
        };

        self.pending_splices.write().await.push(request.clone());

        info!("Splice-in request created: {}", splice_id);

        Ok(request)
    }

    /// Request a splice-out operation (decrease capacity, withdraw on-chain)
    pub async fn request_splice_out(
        &self,
        channel_id: ChannelId,
        amount: u64,
        address: String,
    ) -> LspResult<SpliceRequest> {
        if !self.config.lsp.enable_splicing {
            return Err(crate::LspError::Validation(
                "Channel splicing is not enabled".to_string(),
            ));
        }

        debug!(
            "Requesting splice-out for channel {} of {} sats to {}",
            channel_id, amount, address
        );

        let splice_id = uuid::Uuid::new_v4().to_string();

        let request = SpliceRequest {
            splice_id: splice_id.clone(),
            channel_id,
            operation: SpliceType::SpliceOut { amount, address },
            status: SpliceStatus::PendingPayment,
            created_at: chrono::Utc::now(),
        };

        self.pending_splices.write().await.push(request.clone());

        info!("Splice-out request created: {}", splice_id);

        Ok(request)
    }

    /// Get a pending splice request
    pub async fn get_splice_request(&self, splice_id: &str) -> Option<SpliceRequest> {
        self.pending_splices
            .read()
            .await
            .iter()
            .find(|s| s.splice_id == splice_id)
            .cloned()
    }

    /// Update splice request status
    pub async fn update_splice_status(
        &self,
        splice_id: &str,
        status: SpliceStatus,
    ) -> LspResult<()> {
        let mut splices = self.pending_splices.write().await;
        if let Some(splice) = splices.iter_mut().find(|s| s.splice_id == splice_id) {
            splice.status = status;
            Ok(())
        } else {
            Err(crate::LspError::Validation(format!(
                "Splice request not found: {}",
                splice_id
            )))
        }
    }

    /// List all pending splice requests
    pub async fn list_pending_splices(&self) -> Vec<SpliceRequest> {
        self.pending_splices.read().await.clone()
    }

    /// Cancel a pending splice request
    pub async fn cancel_splice(&self, splice_id: &str) -> LspResult<()> {
        let mut splices = self.pending_splices.write().await;
        if let Some(pos) = splices.iter().position(|s| s.splice_id == splice_id) {
            splices.remove(pos);
            info!("Splice request cancelled: {}", splice_id);
            Ok(())
        } else {
            Err(crate::LspError::Validation(format!(
                "Splice request not found: {}",
                splice_id
            )))
        }
    }
}

/// A splice request
#[derive(Debug, Clone)]
pub struct SpliceRequest {
    /// Unique splice ID
    pub splice_id: String,
    /// Channel ID to splice
    pub channel_id: ChannelId,
    /// Splice operation type
    pub operation: SpliceType,
    /// Current status
    pub status: SpliceStatus,
    /// Creation time
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Type of splice operation
#[derive(Debug, Clone)]
pub enum SpliceType {
    /// Splice in (add funds to channel)
    SpliceIn {
        /// Amount to add
        additional_capacity: u64,
    },
    /// Splice out (remove funds from channel to on-chain)
    SpliceOut {
        /// Amount to remove
        amount: u64,
        /// Bitcoin address to send to
        address: String,
    },
}

/// Splice operation status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpliceStatus {
    /// Waiting for payment
    PendingPayment,
    /// Payment received, preparing splice
    Preparing,
    /// Splice transaction broadcast
    Broadcasting,
    /// Waiting for confirmations
    Confirming,
    /// Splice complete
    Complete,
    /// Splice failed
    Failed,
}
