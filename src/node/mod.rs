//! LDK Node integration module
//!
//! This module provides a wrapper around the LDK Server client for LSP operations.
//! It handles the gRPC connection to ldk-server and provides higher-level LSP-specific
//! functionality.

use crate::{config::Config, LspError, LspResult};
use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

mod client;
mod events;

pub use client::NodeClient;
pub use events::EventHandler;

/// The LSP node that wraps the ldk-server client
pub struct LspNode {
    /// Configuration
    config: Arc<Config>,
    /// gRPC client for ldk-server
    client: Option<NodeClient>,
    /// Event handler for node events
    event_handler: EventHandler,
}

impl LspNode {
    /// Create a new LSP node instance
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        info!("Creating LSP node...");

        let event_handler = EventHandler::new();

        Ok(Self {
            config,
            client: None,
            event_handler,
        })
    }

    /// Start the LSP node and connect to ldk-server
    pub async fn start(&mut self) -> Result<()> {
        info!(
            "Connecting to LDK Server at {}...",
            self.config.ldk_server_address()
        );

        // Connect to ldk-server
        let client = NodeClient::connect(&self.config.ldk_server).await?;
        self.client = Some(client);

        info!("Successfully connected to LDK Server");

        // Start event handling
        self.event_handler.start().await?;

        Ok(())
    }

    /// Stop the LSP node gracefully
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping LSP node...");

        self.event_handler.stop().await?;

        if let Some(client) = self.client.take() {
            client.disconnect().await?;
        }

        info!("LSP node stopped");
        Ok(())
    }

    /// Get the node client if connected
    pub fn client(&self) -> LspResult<&NodeClient> {
        self.client
            .as_ref()
            .ok_or_else(|| LspError::Node("Not connected to LDK Server".to_string()))
    }

    /// Get node info from ldk-server
    pub async fn get_node_info(&self) -> LspResult<NodeInfo> {
        let client = self.client()?;
        client.get_node_info().await
    }

    /// Open a channel to a peer
    pub async fn open_channel(
        &self,
        node_id: &str,
        address: &str,
        capacity: u64,
        push_msat: u64,
        announce_channel: bool,
    ) -> LspResult<ChannelId> {
        let client = self.client()?;

        debug!(
            "Opening channel to {} at {} with capacity {} sat",
            node_id, address, capacity
        );

        client
            .open_channel(node_id, address, capacity, push_msat, announce_channel)
            .await
    }

    /// Accept a channel request (for zeroconf)
    pub async fn accept_channel(&self, channel_id: &ChannelId) -> LspResult<()> {
        let client = self.client()?;

        debug!("Accepting channel {}", channel_id);

        client.accept_channel(channel_id).await
    }

    /// Close a channel
    pub async fn close_channel(
        &self,
        channel_id: &ChannelId,
        force: bool,
    ) -> LspResult<()> {
        let client = self.client()?;

        debug!("Closing channel {} (force={})", channel_id, force);

        client.close_channel(channel_id, force).await
    }

    /// List all channels
    pub async fn list_channels(&self) -> LspResult<Vec<ChannelInfo>> {
        let client = self.client()?;
        client.list_channels().await
    }

    /// Get a specific channel by user_channel_id (short version stored in DB)
    pub async fn get_channel(&self, user_channel_id: &ChannelId) -> LspResult<Option<ChannelInfo>> {
        let channels = self.list_channels().await?;
        Ok(channels.into_iter().find(|c| c.user_channel_id == user_channel_id.0))
    }

    /// Get channels by counterparty node ID
    pub async fn get_channels_by_node_id(&self, node_id: &str) -> LspResult<Vec<ChannelInfo>> {
        let channels = self.list_channels().await?;
        let filtered: Vec<ChannelInfo> = channels
            .into_iter()
            .filter(|c| c.counterparty_node_id == node_id)
            .collect();
        Ok(filtered)
    }

    /// Create an invoice
    pub async fn create_invoice(
        &self,
        amount_msat: Option<u64>,
        description: &str,
        expiry_secs: u32,
    ) -> LspResult<String> {
        let client = self.client()?;
        client.create_invoice(amount_msat, description, expiry_secs).await
    }

    /// Pay an invoice
    pub async fn pay_invoice(&self, invoice: &str) -> LspResult<PaymentId> {
        let client = self.client()?;
        client.pay_invoice(invoice).await
    }

    /// Get payment status
    pub async fn get_payment(&self, payment_id: &PaymentId) -> LspResult<Option<PaymentInfo>> {
        let client = self.client()?;
        client.get_payment(payment_id).await
    }

    /// Splice in funds to increase channel capacity
    pub async fn splice_in(
        &self,
        user_channel_id: &str,
        counterparty_node_id: &str,
        splice_amount_sats: u64,
    ) -> LspResult<()> {
        let client = self.client()?;
        client.splice_in(user_channel_id, counterparty_node_id, splice_amount_sats).await
    }
}

/// Node information
#[derive(Debug, Clone)]
pub struct NodeInfo {
    /// Node ID (public key)
    pub node_id: String,
    /// Network the node is running on
    pub network: String,
    /// Current block height
    pub block_height: u32,
    /// Number of peers connected
    pub num_peers: usize,
    /// Number of channels
    pub num_channels: usize,
}

/// Channel identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChannelId(pub String);

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Channel information
#[derive(Debug, Clone)]
pub struct ChannelInfo {
    /// Channel ID
    pub id: ChannelId,
    /// User channel ID (local identifier assigned when opening)
    pub user_channel_id: String,
    /// Counterparty node ID
    pub counterparty_node_id: String,
    /// Channel capacity in satoshis
    pub capacity_sat: u64,
    /// Local balance in satoshis
    pub local_balance_sat: u64,
    /// Remote balance in satoshis
    pub remote_balance_sat: u64,
    /// Channel is outbound (we opened it)
    pub is_outbound: bool,
    /// Channel is public (announced to network)
    pub is_public: bool,
    /// Channel is ready for payments
    pub is_ready: bool,
    /// Number of confirmations
    pub confirmations: u32,
    /// Required confirmations
    pub required_confirmations: Option<u32>,
}

/// Payment identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PaymentId(pub String);

impl std::fmt::Display for PaymentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Payment information
#[derive(Debug, Clone)]
pub struct PaymentInfo {
    /// Payment ID
    pub id: PaymentId,
    /// Payment hash
    pub payment_hash: String,
    /// Amount in millisatoshis
    pub amount_msat: Option<u64>,
    /// Payment status
    pub status: PaymentStatus,
    /// Payment direction (send/receive)
    pub direction: PaymentDirection,
}

/// Payment status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentStatus {
    /// Payment is pending
    Pending,
    /// Payment succeeded
    Succeeded,
    /// Payment failed
    Failed,
}

/// Payment direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentDirection {
    /// We are sending this payment
    Outbound,
    /// We are receiving this payment
    Inbound,
}