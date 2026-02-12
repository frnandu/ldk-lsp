//! LDK Server gRPC client
//!
//! This module provides a client for communicating with ldk-server via gRPC.

use crate::{
    config::LdkServerConfig,
    node::{ChannelId, ChannelInfo, NodeInfo, PaymentDirection, PaymentId, PaymentInfo, PaymentStatus},
    LspError, LspResult,
};
use tracing::{debug, error, info};

/// Re-export ldk-server-client types
use ldk_server_client::client::LdkServerClient;
use ldk_server_protos::api::{
    Bolt11ReceiveRequest, Bolt11ReceiveResponse, Bolt11SendRequest,
    CloseChannelRequest, ConnectPeerRequest, GetNodeInfoRequest,
    ListChannelsRequest, OpenChannelRequest,
};
use ldk_server_protos::types::{Bolt11InvoiceDescription, bolt11_invoice_description::Kind};

/// Error conversion from LdkServerError to LspError
fn map_client_error(e: ldk_server_client::error::LdkServerError) -> LspError {
    LspError::Grpc(format!("LdkServer error: {:?}", e))
}

/// gRPC client for LDK Server
pub struct NodeClient {
    /// The underlying LDK Server client
    client: LdkServerClient,
    /// Server address
    address: String,
}

impl NodeClient {
    /// Connect to ldk-server
    pub async fn connect(config: &LdkServerConfig) -> Result<Self, LspError> {
        let address = format!("{}:{}", config.host, config.port);
        let base_url = format!("{}:{}", config.host, config.port);

        info!("Connecting to LDK Server at {}", address);

        // Read TLS certificate if provided
        let cert_pem = if let Some(cert_path) = &config.tls_certificate {
            debug!("Using TLS certificate: {:?}", cert_path);
            tokio::fs::read(cert_path)
                .await
                .map_err(|e| LspError::Grpc(format!("Failed to read TLS certificate: {}", e)))?
        } else {
            // Try to read default certificate from data directory
            vec![]
        };

        // Get API key: prefer api_key_file, then auth_token, then empty string
        let api_key = if let Some(api_key_path) = &config.api_key_file {
            debug!("Reading API key from file: {:?}", api_key_path);
            let bytes = tokio::fs::read(api_key_path)
                .await
                .map_err(|e| LspError::Grpc(format!("Failed to read API key file: {}", e)))?;
            
            // The API key file might be binary (raw bytes) or text (hex string)
            // Try to parse as UTF-8 first
            let key = String::from_utf8(bytes.clone())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| {
                    // If not valid UTF-8, hex encode the raw bytes
                    // This handles binary API key files
                    hex::encode(&bytes)
                });
            
            debug!("Loaded API key ({} bytes)", key.len());
            key
        } else {
            config.auth_token.clone().unwrap_or_default()
        };

        // Create the LDK Server client
        let client = LdkServerClient::new(base_url.clone(), api_key, &cert_pem)
            .map_err(|e| LspError::Grpc(format!("Failed to create LDK Server client: {}", e)))?;

        info!("Successfully connected to LDK Server");

        Ok(Self { client, address })
    }

    /// Disconnect from ldk-server
    pub async fn disconnect(self) -> Result<(), LspError> {
        info!("Disconnecting from LDK Server");
        // The client will be dropped, closing the connection
        Ok(())
    }

    /// Get node information
    pub async fn get_node_info(&self) -> LspResult<NodeInfo> {
        debug!("Getting node info");

        let request = GetNodeInfoRequest::default();
        let response = self.client.get_node_info(request).await
            .map_err(map_client_error)?;

        Ok(NodeInfo {
            node_id: response.node_id,
            network: "unknown".to_string(), // GetNodeInfoResponse doesn't have network field
            block_height: response.current_best_block.map(|b| b.height).unwrap_or(0),
            num_peers: 0, // TODO: Get from response when available
            num_channels: 0, // TODO: Get from response when available
        })
    }

    /// Open a channel to a peer
    pub async fn open_channel(
        &self,
        node_id: &str,
        address: &str,
        capacity_sat: u64,
        push_msat: u64,
        announce_channel: bool,
    ) -> LspResult<ChannelId> {
        info!(
            "Opening channel: node_id={}, address={}, capacity={} sats, push={} msat, announce={}",
            node_id, address, capacity_sat, push_msat, announce_channel
        );

        let request = OpenChannelRequest {
            node_pubkey: node_id.to_string(),
            address: address.to_string(),
            channel_amount_sats: capacity_sat,
            push_to_counterparty_msat: Some(push_msat),
            announce_channel,
            ..Default::default()
        };

        let response = self.client.open_channel(request).await
            .map_err(map_client_error)?;

        info!("Channel open initiated: user_channel_id={}", response.user_channel_id);

        // OpenChannelResponse has user_channel_id field, not channel_id
        Ok(ChannelId(response.user_channel_id))
    }

    /// Accept an incoming channel request
    pub async fn accept_channel(&self, _channel_id: &ChannelId) -> LspResult<()> {
        debug!("Accepting channel");
        // Note: ldk-server doesn't have a direct accept_channel endpoint
        // This would typically be handled via events or a different mechanism
        Ok(())
    }

    /// Close a channel
    pub async fn close_channel(&self, channel_id: &ChannelId, force: bool) -> LspResult<()> {
        info!("Closing channel: channel_id={}, force={}", channel_id, force);

        // TODO: To properly close a channel, we need to look up the channel details
        // to get the counterparty_node_id. For now, this is a placeholder.
        // CloseChannelRequest requires:
        // - user_channel_id: String
        // - counterparty_node_id: String
        
        let request = CloseChannelRequest {
            user_channel_id: channel_id.0.clone(),
            counterparty_node_id: String::new(), // TODO: Need to get this from channel list
        };

        self.client.close_channel(request).await
            .map_err(map_client_error)?;

        info!("Channel close initiated: channel_id={}", channel_id);

        Ok(())
    }

    /// Splice in funds to increase channel capacity
    pub async fn splice_in(
        &self,
        user_channel_id: &str,
        counterparty_node_id: &str,
        splice_amount_sats: u64,
    ) -> LspResult<()> {
        info!(
            "Splicing in: user_channel_id={}, counterparty_node_id={}, amount={} sats",
            user_channel_id, counterparty_node_id, splice_amount_sats
        );

        let request = ldk_server_protos::api::SpliceInRequest {
            user_channel_id: user_channel_id.to_string(),
            counterparty_node_id: counterparty_node_id.to_string(),
            splice_amount_sats,
        };

        self.client.splice_in(request).await
            .map_err(map_client_error)?;

        info!("Splice in initiated: user_channel_id={}", user_channel_id);

        Ok(())
    }

    /// List all channels
    pub async fn list_channels(&self) -> LspResult<Vec<ChannelInfo>> {
        debug!("Listing channels");

        let request = ListChannelsRequest::default();
        let response = self.client.list_channels(request).await
            .map_err(map_client_error)?;

        let channels = response.channels.into_iter().map(|c| {
            // Calculate local balance from outbound_capacity_msat
            let local_balance_sat = c.outbound_capacity_msat / 1000;
            let remote_balance_sat = c.inbound_capacity_msat / 1000;
            
            ChannelInfo {
                id: ChannelId(c.channel_id),
                user_channel_id: c.user_channel_id,
                counterparty_node_id: c.counterparty_node_id,
                capacity_sat: c.channel_value_sats,
                local_balance_sat,
                remote_balance_sat,
                is_outbound: c.is_outbound,
                is_public: c.is_announced, // Channel has is_announced, not is_public
                is_ready: c.is_channel_ready, // Channel has is_channel_ready, not is_ready
                confirmations: c.confirmations.unwrap_or(0),
                required_confirmations: c.confirmations_required,
            }
        }).collect();

        Ok(channels)
    }

    /// Get channels by counterparty node ID
    pub async fn get_channels_by_node_id(&self, node_id: &str) -> LspResult<Vec<ChannelInfo>> {
        debug!("Getting channels for node_id: {}", node_id);

        let all_channels = self.list_channels().await?;
        let filtered: Vec<ChannelInfo> = all_channels
            .into_iter()
            .filter(|c| c.counterparty_node_id == node_id)
            .collect();

        debug!(
            "Found {} channels for node_id: {}",
            filtered.len(),
            node_id
        );
        Ok(filtered)
    }

    /// Create a new invoice using ldk-server's bolt11_receive endpoint
    pub async fn create_invoice(
        &self,
        amount_msat: Option<u64>,
        description: &str,
        expiry_secs: u32,
    ) -> LspResult<String> {
        debug!(
            "Creating invoice (amount: {:?}, description: {}, expiry: {})",
            amount_msat, description, expiry_secs
        );

        // Create the invoice description using the correct proto structure
        let description = if description.is_empty() {
            None
        } else {
            Some(Bolt11InvoiceDescription {
                kind: Some(Kind::Direct(description.to_string())),
            })
        };

        // Build the request using the exact field names from ldk-server-protos
        let request = Bolt11ReceiveRequest {
            amount_msat,
            description,
            expiry_secs,
        };

        // Call the ldk-server bolt11_receive endpoint
        let response: Bolt11ReceiveResponse = self.client.bolt11_receive(request).await
            .map_err(|e| {
                error!("Failed to create invoice via ldk-server: {:?}", e);
                map_client_error(e)
            })?;

        info!("Successfully created invoice via ldk-server");
        Ok(response.invoice)
    }

    /// Pay an invoice
    pub async fn pay_invoice(&self, invoice: &str) -> LspResult<PaymentId> {
        info!("Paying invoice: {}", invoice);

        let request = Bolt11SendRequest {
            invoice: invoice.to_string(),
            ..Default::default()
        };

        let response = self.client.bolt11_send(request).await
            .map_err(map_client_error)?;

        info!("Payment initiated: payment_id={}", response.payment_id);

        Ok(PaymentId(response.payment_id))
    }

    /// Get payment information
    pub async fn get_payment(&self, payment_id: &PaymentId) -> LspResult<Option<PaymentInfo>> {
        debug!("Getting payment info for {}", payment_id);

        let request = ldk_server_protos::api::GetPaymentDetailsRequest {
            payment_id: payment_id.0.clone(),
        };

        let response = self.client.get_payment_details(request).await
            .map_err(map_client_error)?;

        if let Some(payment) = response.payment {
            // Convert i32 to PaymentStatus using from_i32
            let status = ldk_server_protos::types::PaymentStatus::from_i32(payment.status)
                .map(|s| match s {
                    ldk_server_protos::types::PaymentStatus::Pending => PaymentStatus::Pending,
                    ldk_server_protos::types::PaymentStatus::Succeeded => PaymentStatus::Succeeded,
                    ldk_server_protos::types::PaymentStatus::Failed => PaymentStatus::Failed,
                })
                .unwrap_or(PaymentStatus::Pending);
            
            // Convert i32 to PaymentDirection using from_i32
            let direction = ldk_server_protos::types::PaymentDirection::from_i32(payment.direction)
                .map(|d| match d {
                    ldk_server_protos::types::PaymentDirection::Outbound => PaymentDirection::Outbound,
                    ldk_server_protos::types::PaymentDirection::Inbound => PaymentDirection::Inbound,
                })
                .unwrap_or(PaymentDirection::Inbound);
            
            Ok(Some(PaymentInfo {
                id: payment_id.clone(),
                payment_hash: payment.id,
                amount_msat: payment.amount_msat,
                status,
                direction,
            }))
        } else {
            Ok(None)
        }
    }

    /// Connect to a peer
    pub async fn connect_peer(&self, node_id: &str, address: &str) -> LspResult<()> {
        info!("Connecting to peer: node_id={}, address={}", node_id, address);

        let request = ConnectPeerRequest {
            node_pubkey: node_id.to_string(),
            address: address.to_string(),
            ..Default::default()
        };

        self.client.connect_peer(request).await
            .map_err(map_client_error)?;

        info!("Peer connected: node_id={}", node_id);

        Ok(())
    }

    /// Disconnect from a peer
    pub async fn disconnect_peer(&self, _node_id: &str) -> LspResult<()> {
        debug!("Disconnecting from peer");
        // Note: ldk-server doesn't have a direct disconnect_peer endpoint
        Ok(())
    }
}
