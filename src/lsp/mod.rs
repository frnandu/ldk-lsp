//! LSP (Lightning Service Provider) core functionality
//!
//! This module implements the core LSP features:
//! - Zero-confirmation channel opens
//! - Channel splicing for inbound capacity
//! - Channel request handling and validation

use crate::{
    config::Config,
    db::{ChannelRequestModel, Database, PaymentModel},
    node::{ChannelId, LspNode, PaymentId},
    LspError, LspResult,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

mod splicing;
mod zeroconf;

pub use splicing::SplicingService;
pub use zeroconf::ZeroconfService;

/// The main LSP service that manages channel operations
pub struct LspService {
    /// Configuration
    config: Arc<Config>,
    /// Database connection
    db: Arc<Database>,
    /// Zero-confirmation channel service
    pub zeroconf: ZeroconfService,
    /// Channel splicing service
    pub splicing: SplicingService,
}

// Re-export types for API
pub use splicing::{SpliceRequest, SpliceStatus, SpliceType};

impl LspService {
    /// Create a new LSP service
    pub fn new(config: Arc<Config>, db: Arc<Database>) -> Self {
        let zeroconf = ZeroconfService::new(config.clone());
        let splicing = SplicingService::new(config.clone());

        Self {
            config,
            db,
            zeroconf,
            splicing,
        }
    }

    /// Initialize the LSP service
    pub async fn init(&self, node: Arc<RwLock<LspNode>>) -> LspResult<()> {
        info!("Initializing LSP service...");

        self.zeroconf.init(node.clone()).await?;
        self.splicing.init(node.clone()).await?;

        // Reconcile pending payments on startup
        self.reconcile_pending_payments(node.clone()).await?;
        
        // Reconcile channels in opening state on startup
        self.reconcile_channel_opening(node.clone()).await?;

        info!("LSP service initialized");
        Ok(())
    }

    /// Reconcile pending payments on startup
    /// Checks all pending_payment requests and:
    /// - Deletes requests that are expired (> 1 hour old)
    /// - Opens channels for requests with received payments
    async fn reconcile_pending_payments(&self, node: Arc<RwLock<LspNode>>) -> LspResult<()> {
        info!("Reconciling pending payments on startup...");

        let queries = crate::db::ChannelRequestQueries::new(&self.db);
        let pending_requests = queries
            .list_by_status("pending_payment")
            .await
            .map_err(|e| LspError::Database(format!("Failed to list pending requests: {}", e)))?;

        if pending_requests.is_empty() {
            info!("No pending payments to reconcile");
            return Ok(());
        }

        let total_requests = pending_requests.len();
        info!("Found {} pending payment requests to reconcile", total_requests);
        let one_hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
        let mut processed_count = 0;
        let mut expired_count = 0;
        let mut channel_opened_count = 0;
        let mut payment_failed_count = 0;

        for request in pending_requests {
            processed_count += 1;
            
            // Check if request is expired (> 1 hour old)
            if request.created_at < one_hour_ago {
                info!(
                    "[Pending Payments {}/{}] Deleting expired request {} (created at {})",
                    processed_count, total_requests, request.id, request.created_at
                );
                expired_count += 1;
                if let Err(e) = queries.delete(&request.id).await {
                    error!("Failed to delete expired request {}: {}", request.id, e);
                }
                continue;
            }

            // Check payment status on ldk-server
            if let Some(ref payment_hash) = request.payment_hash {
                info!("[Pending Payments {}/{}] Checking payment {} for request {}", 
                    processed_count, total_requests, payment_hash, request.id);
                
                let payment = {
                    let node = node.read().await;
                    node.get_payment(&PaymentId(payment_hash.clone())).await
                };

                match payment {
                    Ok(Some(payment)) => {
                        match payment.status {
                            crate::node::PaymentStatus::Succeeded => {
                                channel_opened_count += 1;
                                info!(
                                    "[Pending Payments {}/{}] Payment received for request {}, opening channel...",
                                    processed_count, total_requests, request.id
                                );

                                // Update status to opening_channel
                                if let Err(e) = queries
                                    .update_status(&request.id, "opening_channel", None)
                                    .await
                                {
                                    error!("Failed to update status for request {}: {}", request.id, e);
                                    continue;
                                }

                                // Open the channel
                                let address = format!("{}:{}", request.host, request.port);
                                match async {
                                    let node = node.read().await;
                                    node.open_channel(
                                        &request.node_id,
                                        &address,
                                        request.capacity as u64,
                                        0,
                                        false,
                                    )
                                    .await
                                }.await {
                                    Ok(cid) => {
                                        info!(
                                            "[Pending Payments {}/{}] Channel opened for request {}: channel_id={}",
                                            processed_count, total_requests, request.id, cid
                                        );
                                        if let Err(e) = queries
                                            .update_status(&request.id, "channel_opened", Some(&cid.0))
                                            .await
                                        {
                                            error!("Failed to update status for request {}: {}", request.id, e);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to open channel for request {}: {}", request.id, e);
                                        if let Err(e) = queries
                                            .update_status_with_reason(
                                                &request.id,
                                                "channel_open_failed",
                                                None,
                                                Some(&format!("Failed to open channel: {}", e)),
                                            )
                                            .await
                                        {
                                            error!("Failed to update failure status for request {}: {}", request.id, e);
                                        }
                                    }
                                }
                            }
                            crate::node::PaymentStatus::Failed => {
                                payment_failed_count += 1;
                                warn!("[Pending Payments {}/{}] Payment failed for request {}", 
                                    processed_count, total_requests, request.id);
                                if let Err(e) = queries
                                    .update_status(&request.id, "payment_failed", None)
                                    .await
                                {
                                    error!("Failed to update status for request {}: {}", request.id, e);
                                }
                            }
                            _ => {
                                info!("[Pending Payments {}/{}] Payment still pending for request {}", 
                                    processed_count, total_requests, request.id);
                            }
                        }
                    }
                    Ok(None) => {
                        info!("[Pending Payments {}/{}] Payment not found on ldk-server for request {}", 
                            processed_count, total_requests, request.id);
                    }
                    Err(e) => {
                        error!("[Pending Payments {}/{}] Failed to get payment for request {}: {}", 
                            processed_count, total_requests, request.id, e);
                    }
                }
            } else {
                warn!("[Pending Payments {}/{}] Request {} has no payment hash, skipping", 
                    processed_count, total_requests, request.id);
            }
        }

        info!("Pending payment reconciliation completed: {} processed, {} expired deleted, {} channels opened, {} payments failed, {} still pending", 
            processed_count, expired_count, channel_opened_count, payment_failed_count, 
            total_requests - processed_count);
        Ok(())
    }

    /// Reconcile channels in opening state on startup
    /// Checks all channel_opening requests and:
    /// - Updates to channel_opened if channel is ready on ldk-server
    /// - Updates to channel_open_failed if channel doesn't exist or is closed
    async fn reconcile_channel_opening(&self, node: Arc<RwLock<LspNode>>) -> LspResult<()> {
        info!("Reconciling channel opening status on startup...");

        let queries = crate::db::ChannelRequestQueries::new(&self.db);
        let opening_requests = queries
            .list_by_status("channel_opening")
            .await
            .map_err(|e| LspError::Database(format!("Failed to list opening channel requests: {}", e)))?;

        if opening_requests.is_empty() {
            info!("No channels in opening state to reconcile");
            return Ok(());
        }

        let total_requests = opening_requests.len();
        info!("Found {} channel opening requests to reconcile", total_requests);
        let mut processed_count = 0;
        let mut ready_count = 0;
        let mut not_ready_count = 0;
        let mut failed_count = 0;
        let mut skipped_recent_count = 0;
        
        // Channels that are still opening should be given time before marked as failed
        // This handles the case where we restart while channel is still being negotiated
        let opening_timeout = chrono::Duration::minutes(5);
        let now = chrono::Utc::now();
        
        // Get all channels from ldk-server
        let channels = {
            let node = node.read().await;
            match node.list_channels().await {
                Ok(channels) => {
                    info!("Retrieved {} channels from ldk-server", channels.len());
                    channels
                }
                Err(e) => {
                    error!("Failed to list channels from ldk-server: {}", e);
                    return Ok(());
                }
            }
        };

        for request in opening_requests {
            processed_count += 1;
            if let Some(ref user_channel_id) = request.channel_id {
                // Look for this channel in ldk-server's channel list
                match channels.iter().find(|c| &c.user_channel_id == user_channel_id) {
                    Some(channel) => {
                        if channel.is_ready {
                            ready_count += 1;
                            info!(
                                "[Channel Opening {}/{}] Channel {} for request {} is ready, updating to channel_opened",
                                processed_count, total_requests, user_channel_id, request.id
                            );
                            if let Err(e) = queries
                                .update_status(&request.id, "channel_opened", Some(user_channel_id))
                                .await
                            {
                                error!("Failed to update status for request {}: {}", request.id, e);
                            }
                        } else {
                            not_ready_count += 1;
                            info!(
                                "[Channel Opening {}/{}] Channel {} for request {} exists but not ready yet",
                                processed_count, total_requests, user_channel_id, request.id
                            );
                        }
                    }
                    None => {
                        // Channel not found in list - check if it's recent or old
                        let time_since_update = now - request.updated_at;
                        if time_since_update < opening_timeout {
                            // Channel is recent, might still be opening - don't mark as failed yet
                            skipped_recent_count += 1;
                            info!(
                                "[Channel Opening {}/{}] Channel {} for request {} not found but recent ({}s ago), skipping - may still be opening",
                                processed_count, total_requests, user_channel_id, request.id, time_since_update.num_seconds()
                            );
                        } else {
                            // Channel is old and not found - likely failed
                            failed_count += 1;
                            warn!(
                                "[Channel Opening {}/{}] Channel {} for request {} not found on ldk-server (last update {}s ago), marking as failed",
                                processed_count, total_requests, user_channel_id, request.id, time_since_update.num_seconds()
                            );
                            if let Err(e) = queries
                                .update_status_with_reason(
                                    &request.id,
                                    "channel_open_failed",
                                    Some(user_channel_id),
                                    Some(&format!("Channel not found on ldk-server after {} seconds", time_since_update.num_seconds())),
                                )
                                .await
                            {
                                error!("Failed to update failure status for request {}: {}", request.id, e);
                            }
                        }
                    }
                }
            } else {
                warn!("[Channel Opening {}/{}] Request {} has no channel_id, cannot reconcile", 
                    processed_count, total_requests, request.id);
            }
        }

        info!("Channel opening reconciliation completed: {} processed, {} channels ready, {} not ready, {} failed/not found, {} skipped (recent)", 
            processed_count, ready_count, not_ready_count, failed_count, skipped_recent_count);
        Ok(())
    }

    /// Request a new channel from the LSP
    /// Creates invoice, saves the request to the database with payment hash in a single operation
    pub async fn request_channel(
        &self,
        node_id: &str,
        host: &str,
        port: u16,
        capacity: u64,
        require_zeroconf: bool,
        node: Arc<RwLock<LspNode>>,
    ) -> LspResult<(ChannelQuote, String)> {
        info!(
            "Channel request from {}@{}:{} for {} sats (zeroconf: {})",
            node_id, host, port, capacity, require_zeroconf
        );

        // Validate the request
        self.validate_channel_request(capacity, require_zeroconf)?;

        // Calculate the fee
        let fee = self.config.calculate_channel_fee(capacity);
        let total_cost = capacity + fee;

        // Generate a request ID
        let request_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        // Create invoice for payment
        let invoice = {
            let node = node.read().await;
            node.create_invoice(
                Some(total_cost * 1000), // Convert to millisatoshis
                &format!("Channel open: {} sats to {}", capacity, node_id),
                600, // 10 minute expiry
            )
            .await
            .map_err(|e| LspError::Node(format!("Failed to create invoice: {}", e)))?
        };

        // Extract payment hash from invoice
        let payment_hash = Self::extract_payment_hash_from_invoice(&invoice)?;

        // Save the request to the database with payment hash included
        let request_model = ChannelRequestModel {
            id: request_id.clone(),
            node_id: node_id.to_string(),
            host: host.to_string(),
            port: port as i32,
            capacity: capacity as i64,
            fee: fee as i64,
            require_zeroconf,
            status: "pending_payment".to_string(),
            channel_id: None,
            payment_hash: Some(payment_hash.clone()),
            failure_reason: None,
            created_at: now,
            updated_at: now,
        };

        let queries = crate::db::ChannelRequestQueries::new(&self.db);
        queries
            .insert(&request_model)
            .await
            .map_err(|e| LspError::Database(format!("Failed to save channel request: {}", e)))?;

        info!(
            "Channel quote generated and saved to DB: request_id={}, capacity={}, fee={}, total={}, payment_hash={}",
            request_id, capacity, fee, total_cost, payment_hash
        );

        let quote = ChannelQuote {
            request_id,
            capacity,
            fee,
            total_cost,
            require_zeroconf,
            lsp_node_id: self.config.node.alias.clone(),
            expiry: now + chrono::Duration::minutes(10),
            payment_hash,
        };

        Ok((quote, invoice))
    }

    /// Extract payment hash from BOLT11 invoice
    fn extract_payment_hash_from_invoice(invoice: &str) -> LspResult<String> {
        let parsed = invoice
            .parse::<lightning_invoice::Bolt11Invoice>()
            .map_err(|e| LspError::Validation(format!("Failed to parse BOLT11 invoice: {}", e)))?;

        // Get the payment hash from the invoice - it's already a sha256::Hash which implements Display as hex
        Ok(parsed.payment_hash().to_string())
    }

    /// Check the status of a channel request
    /// If payment is detected on ldk-server, opens the channel
    pub async fn check_request_status(
        &self,
        request_id: &str,
        node: Arc<RwLock<LspNode>>,
    ) -> LspResult<ChannelRequestStatus> {
        debug!("Checking status for request: {}", request_id);

        // Get request from database
        let queries = crate::db::ChannelRequestQueries::new(&self.db);
        let request = queries
            .get_by_id(request_id)
            .await
            .map_err(|e| LspError::Database(format!("Failed to get request: {}", e)))?
            .ok_or_else(|| LspError::Validation(format!("Channel request not found: {}", request_id)))?;

        // Check current status
        match request.status.as_str() {
            "channel_opened" => {
                if let Some(channel_id) = request.channel_id {
                    return Ok(ChannelRequestStatus::ChannelOpened(ChannelId(channel_id)));
                }
            }
            "opening_channel" => return Ok(ChannelRequestStatus::OpeningChannel),
            "expired" => return Ok(ChannelRequestStatus::Expired),
            "cancelled" => return Ok(ChannelRequestStatus::Cancelled),
            _ => {} // pending_payment - check if paid
        }

        // If pending payment, check if invoice is paid on ldk-server
        if let Some(ref payment_hash) = request.payment_hash {
            let payment = {
                let node = node.read().await;
                node.get_payment(&PaymentId(payment_hash.clone())).await?
            };

            if let Some(payment) = payment {
                match payment.status {
                    crate::node::PaymentStatus::Succeeded => {
                        info!(
                            "Payment received for request {}, opening channel...",
                            request_id
                        );

                        // Update status to opening_channel
                        queries
                            .update_status(request_id, "opening_channel", None)
                            .await
                            .map_err(|e| LspError::Database(format!("Failed to update status: {}", e)))?;

                        // Open the channel
                        let address = format!("{}:{}", request.host, request.port);
                        let channel_id = {
                            let node = node.read().await;
                            node.open_channel(
                                &request.node_id,
                                &address,
                                request.capacity as u64,
                                0,     // No push MSAT
                                false, // Private channel (not announced)
                            )
                            .await?
                        };

                        // Update status to channel_opened
                        queries
                            .update_status(request_id, "channel_opened", Some(&channel_id.0))
                            .await
                            .map_err(|e| LspError::Database(format!("Failed to update status: {}", e)))?;

                        info!(
                            "Channel opened for request {}: channel_id={}",
                            request_id, channel_id
                        );

                        return Ok(ChannelRequestStatus::ChannelOpened(channel_id));
                    }
                    crate::node::PaymentStatus::Failed => {
                        warn!("Payment failed for request {}", request_id);
                        queries
                            .update_status(request_id, "payment_failed", None)
                            .await
                            .map_err(|e| LspError::Database(format!("Failed to update status: {}", e)))?;
                        return Err(LspError::Validation("Payment failed".to_string()));
                    }
                    _ => {
                        debug!("Payment still pending for request {}", request_id);
                    }
                }
            } else {
                debug!("Payment not found on ldk-server for request {}", request_id);
            }
        }

        // If payment_hash is missing, the request is corrupted
        if request.payment_hash.is_none() {
            return Err(LspError::Database(
                "Channel request has no payment hash".to_string(),
            ));
        }

        // Check if expired
        let expiry = request.created_at + chrono::Duration::minutes(10);
        if chrono::Utc::now() > expiry {
            queries
                .update_status(request_id, "expired", None)
                .await
                .map_err(|e| LspError::Database(format!("Failed to update status: {}", e)))?;
            return Ok(ChannelRequestStatus::Expired);
        }

        Ok(ChannelRequestStatus::PendingPayment)
    }

    /// Get a channel request by ID
    pub async fn get_request(&self, request_id: &str) -> LspResult<Option<ChannelRequest>> {
        let queries = crate::db::ChannelRequestQueries::new(&self.db);
        let model = queries
            .get_by_id(request_id)
            .await
            .map_err(|e| LspError::Database(format!("Failed to get request: {}", e)))?;

        Ok(model.map(|m| ChannelRequest {
            id: m.id,
            node_id: m.node_id,
            host: m.host,
            port: m.port as u16,
            capacity: m.capacity as u64,
            fee: m.fee as u64,
            require_zeroconf: m.require_zeroconf,
            status: match m.status.as_str() {
                "pending_payment" => ChannelRequestStatus::PendingPayment,
                "opening_channel" => ChannelRequestStatus::OpeningChannel,
                "channel_opened" => {
                    if let Some(cid) = m.channel_id {
                        ChannelRequestStatus::ChannelOpened(ChannelId(cid))
                    } else {
                        ChannelRequestStatus::PendingPayment
                    }
                }
                "expired" => ChannelRequestStatus::Expired,
                "cancelled" => ChannelRequestStatus::Cancelled,
                _ => ChannelRequestStatus::PendingPayment,
            },
            payment_hash: m.payment_hash,
            created_at: m.created_at,
        }))
    }

    /// Handle a splice request to increase channel capacity
    pub async fn request_splice(
        &self,
        channel_id: &ChannelId,
        additional_capacity: u64,
        node: Arc<RwLock<LspNode>>,
    ) -> LspResult<(SpliceQuote, String)> {
        if !self.config.lsp.enable_splicing {
            return Err(LspError::Validation(
                "Splicing is not enabled on this LSP".to_string(),
            ));
        }

        info!(
            "Splice request for channel {} with additional capacity {}",
            channel_id, additional_capacity
        );

        // Validate the splice request
        if additional_capacity < self.config.lsp.min_channel_size {
            return Err(LspError::Validation(format!(
                "Additional capacity must be at least {} sat",
                self.config.lsp.min_channel_size
            )));
        }

        // Calculate the fee
        let fee = self.config.calculate_channel_fee(additional_capacity);
        let total_cost = additional_capacity + fee;

        // Verify the channel exists
        let channel = {
            let node = node.read().await;
            node.get_channel(channel_id).await?
        };

        if channel.is_none() {
            return Err(LspError::Channel(format!(
                "Channel {} not found",
                channel_id
            )));
        }

        let splice_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        // Create invoice for payment
        let invoice = {
            let node = node.read().await;
            node.create_invoice(
                Some(total_cost * 1000), // Convert to millisatoshis
                &format!("Splice in: {} sats to channel {}", additional_capacity, channel_id),
                600, // 10 minute expiry
            )
            .await
            .map_err(|e| LspError::Node(format!("Failed to create invoice: {}", e)))?
        };

        // Extract payment hash from invoice
        let payment_hash = Self::extract_payment_hash_from_invoice(&invoice)?;

        // Save the splice request to the database
        let splice_model = crate::db::SpliceRequestModel {
            id: splice_id.clone(),
            channel_id: channel_id.to_string(),
            operation_type: "splice_in".to_string(),
            amount: additional_capacity as i64,
            address: None,
            status: "pending_payment".to_string(),
            txid: None,
            payment_hash: Some(payment_hash.clone()),
            created_at: now,
            updated_at: now,
        };

        let queries = crate::db::SpliceRequestQueries::new(&self.db);
        queries
            .insert(&splice_model)
            .await
            .map_err(|e| LspError::Database(format!("Failed to save splice request: {}", e)))?;

        info!(
            "Splice quote generated and saved to DB: splice_id={}, additional_capacity={}, fee={}, total={}, payment_hash={}",
            splice_id, additional_capacity, fee, total_cost, payment_hash
        );

        let quote = SpliceQuote {
            splice_id,
            channel_id: channel_id.clone(),
            additional_capacity,
            fee,
            total_cost,
            expiry: now + chrono::Duration::minutes(10),
            payment_hash,
        };

        Ok((quote, invoice))
    }

    /// Validate a channel request
    fn validate_channel_request(&self, capacity: u64, require_zeroconf: bool) -> LspResult<()> {
        // Check minimum and maximum capacity
        if capacity < self.config.lsp.min_channel_size {
            return Err(LspError::Validation(format!(
                "Channel capacity {} is below minimum {}",
                capacity, self.config.lsp.min_channel_size
            )));
        }

        if capacity > self.config.lsp.max_channel_size {
            return Err(LspError::Validation(format!(
                "Channel capacity {} exceeds maximum {}",
                capacity, self.config.lsp.max_channel_size
            )));
        }

        // Check zeroconf requirements
        if require_zeroconf && !self.config.lsp.enable_zeroconf {
            return Err(LspError::Validation(
                "Zero-confirmation channels are not enabled on this LSP".to_string(),
            ));
        }

        if require_zeroconf && capacity < self.config.lsp.zeroconf_min_size {
            return Err(LspError::Validation(format!(
                "Zero-confirmation channels require minimum capacity of {} sat",
                self.config.lsp.zeroconf_min_size
            )));
        }

        Ok(())
    }

    /// Get a splice request by ID (delegates to splicing service)
    pub async fn get_splice_request(&self, splice_id: &str) -> Option<crate::lsp::SpliceRequest> {
        self.splicing.get_splice_request(splice_id).await
    }
}

/// A channel quote for a client
#[derive(Debug, Clone)]
pub struct ChannelQuote {
    /// Unique request ID
    pub request_id: String,
    /// Requested channel capacity
    pub capacity: u64,
    /// LSP fee
    pub fee: u64,
    /// Total cost (capacity + fee)
    pub total_cost: u64,
    /// Whether zero-confirmation is requested
    pub require_zeroconf: bool,
    /// LSP node ID
    pub lsp_node_id: String,
    /// Quote expiry time
    pub expiry: chrono::DateTime<chrono::Utc>,
    /// Payment hash for the invoice
    pub payment_hash: String,
}

/// A splice quote for increasing channel capacity
#[derive(Debug, Clone)]
pub struct SpliceQuote {
    /// Unique splice ID
    pub splice_id: String,
    /// Channel ID to splice
    pub channel_id: ChannelId,
    /// Additional capacity requested
    pub additional_capacity: u64,
    /// LSP fee
    pub fee: u64,
    /// Total cost
    pub total_cost: u64,
    /// Quote expiry time
    pub expiry: chrono::DateTime<chrono::Utc>,
    /// Payment hash for the invoice
    pub payment_hash: String,
}

/// A channel request
#[derive(Debug, Clone)]
pub struct ChannelRequest {
    /// Unique request ID
    pub id: String,
    /// Client node ID
    pub node_id: String,
    /// Client host
    pub host: String,
    /// Client port
    pub port: u16,
    /// Requested capacity
    pub capacity: u64,
    /// LSP fee
    pub fee: u64,
    /// Whether zero-confirmation is requested
    pub require_zeroconf: bool,
    /// Request status
    pub status: ChannelRequestStatus,
    /// Payment hash for the invoice
    pub payment_hash: Option<String>,
    /// Creation time
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Channel request status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelRequestStatus {
    /// Waiting for payment
    PendingPayment,
    /// Payment received, opening channel
    OpeningChannel,
    /// Channel opened
    ChannelOpened(ChannelId),
    /// Request expired
    Expired,
    /// Request cancelled
    Cancelled,
}
