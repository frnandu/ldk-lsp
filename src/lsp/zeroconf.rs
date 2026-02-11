//! Zero-confirmation channel service
//!
//! Zero-confirmation channels allow clients to receive payments immediately
//! without waiting for on-chain confirmations. This requires trust between
//! the LSP and the client.

use crate::{
    config::Config,
    node::{ChannelId, LspNode},
    LspResult,
};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Service for managing zero-confirmation channels
pub struct ZeroconfService {
    /// Configuration
    config: Arc<Config>,
    /// Set of channels that are zeroconf-enabled
    zeroconf_channels: Arc<RwLock<HashSet<ChannelId>>>,
    /// Pending zeroconf channel count per peer
    pending_count: Arc<RwLock<std::collections::HashMap<String, u32>>>,
}

impl ZeroconfService {
    /// Create a new zeroconf service
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            zeroconf_channels: Arc::new(RwLock::new(HashSet::new())),
            pending_count: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Initialize the zeroconf service
    pub async fn init(&self, _node: Arc<RwLock<LspNode>>) -> LspResult<()> {
        info!("Initializing zeroconf service...");

        if !self.config.lsp.enable_zeroconf {
            info!("Zero-confirmation channels are disabled");
            return Ok(());
        }

        info!("Zero-confirmation service initialized");
        Ok(())
    }

    /// Register a channel as zeroconf-enabled
    pub async fn register_zeroconf_channel(&self, channel_id: ChannelId) -> LspResult<()> {
        if !self.config.lsp.enable_zeroconf {
            return Err(crate::LspError::Validation(
                "Zero-confirmation channels are not enabled".to_string(),
            ));
        }

        debug!("Registering zeroconf channel: {}", channel_id);
        self.zeroconf_channels.write().await.insert(channel_id);
        Ok(())
    }

    /// Check if a channel is zeroconf-enabled
    pub async fn is_zeroconf_channel(&self, channel_id: &ChannelId) -> bool {
        self.zeroconf_channels.read().await.contains(channel_id)
    }

    /// Unregister a zeroconf channel (e.g., after it confirms)
    pub async fn unregister_zeroconf_channel(&self, channel_id: &ChannelId) {
        debug!("Unregistering zeroconf channel: {}", channel_id);
        self.zeroconf_channels.write().await.remove(channel_id);
    }

    /// Check if a peer can open a new zeroconf channel
    pub async fn can_open_zeroconf(&self, node_id: &str) -> bool {
        let count = self.pending_count.read().await.get(node_id).copied().unwrap_or(0);
        count < self.config.lsp.max_pending_zeroconf
    }

    /// Increment pending zeroconf count for a peer
    pub async fn increment_pending(&self, node_id: &str) {
        let mut counts = self.pending_count.write().await;
        let count = counts.entry(node_id.to_string()).or_insert(0);
        *count += 1;
    }

    /// Decrement pending zeroconf count for a peer
    pub async fn decrement_pending(&self, node_id: &str) {
        let mut counts = self.pending_count.write().await;
        if let Some(count) = counts.get_mut(node_id) {
            if *count > 0 {
                *count -= 1;
            }
            if *count == 0 {
                counts.remove(node_id);
            }
        }
    }

    /// Get the list of zeroconf channels
    pub async fn get_zeroconf_channels(&self) -> Vec<ChannelId> {
        self.zeroconf_channels.read().await.iter().cloned().collect()
    }
}

/// Trust score for a peer regarding zeroconf channels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustScore {
    /// New peer with no history
    New,
    /// Peer with some history
    Established,
    /// Highly trusted peer
    Trusted,
    /// Peer has violated trust
    Untrusted,
}

impl TrustScore {
    /// Get the maximum zeroconf channel value for this trust level
    pub fn max_channel_value(&self, config: &Config) -> u64 {
        match self {
            TrustScore::New => config.lsp.zeroconf_min_size,
            TrustScore::Established => config.lsp.min_channel_size * 5,
            TrustScore::Trusted => config.lsp.max_channel_size,
            TrustScore::Untrusted => 0,
        }
    }
}