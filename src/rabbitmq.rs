//! RabbitMQ event consumer for ldk-server events
//!
//! This module consumes events from ldk-server's RabbitMQ exchange
//! and triggers appropriate actions (like opening channels when payments are received).

use crate::{
    config::RabbitMqConfig,
    db::{Database, ReceiveRequestQueries},
    node::{ChannelId, LspNode, PaymentId},
    LspResult,
};
use lapin::{
    options::*,
    types::FieldTable,
    BasicProperties, Connection, ConnectionProperties, Consumer,
};
use ldk_server_protos::events::{event_envelope::Event, EventEnvelope};
use prost::Message;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// RabbitMQ event consumer
pub struct RabbitMqConsumer {
    /// Configuration
    config: RabbitMqConfig,
    /// Database connection
    db: Arc<Database>,
    /// LSP node for opening channels
    node: Arc<RwLock<LspNode>>,
    /// Consumer instance
    consumer: Option<Consumer>,
}

impl RabbitMqConsumer {
    /// Create a new RabbitMQ consumer
    pub fn new(
        config: RabbitMqConfig,
        db: Arc<Database>,
        node: Arc<RwLock<LspNode>>,
    ) -> Self {
        Self {
            config,
            db,
            node,
            consumer: None,
        }
    }

    /// Start consuming events from RabbitMQ
    pub async fn start(&mut self) -> anyhow::Result<()> {
        info!(
            "Connecting to RabbitMQ at {}...",
            self.config.connection_string
        );

        // Connect to RabbitMQ
        let conn = Connection::connect(
            &self.config.connection_string,
            ConnectionProperties::default(),
        )
        .await?;

        info!("Connected to RabbitMQ");

        // Create channel
        let channel = conn.create_channel().await?;

        // Declare the exchange (should match ldk-server's exchange)
        channel
            .exchange_declare(
                &self.config.exchange_name,
                lapin::ExchangeKind::Fanout,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        // Use configured queue name or default to a fixed name for persistence
        let queue_name = self
            .config
            .queue_name
            .clone()
            .unwrap_or_else(|| "ldk-lsp-events".to_string());

        // Declare queue
        let queue = channel
            .queue_declare(
                &queue_name,
                QueueDeclareOptions {
                    durable: true,
                    auto_delete: false,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        info!("Declared queue: {}", queue_name);

        // Bind queue to exchange
        // For fanout exchanges, all bound queues receive all messages
        let routing_key = "";
        channel
            .queue_bind(
                &queue_name,
                &self.config.exchange_name,
                routing_key,
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await?;
        debug!("Bound queue to exchange with routing key: {}", routing_key);

        // Start consuming
        let consumer = channel
            .basic_consume(
                &queue_name,
                "ldk-lsp-consumer",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        info!("Started consuming events from RabbitMQ");
        self.consumer = Some(consumer);

        // Spawn event processing task
        let db = self.db.clone();
        let node = self.node.clone();
        let mut consumer = self.consumer.take().unwrap();

        tokio::spawn(async move {
            while let Some(delivery) = consumer.next().await {
                match delivery {
                    Ok(delivery) => {
                        if let Err(e) = Self::process_event(&db, &node, &delivery).await {
                            error!("Failed to process event: {}", e);
                        }
                        
                        // Acknowledge the message
                        if let Err(e) = delivery
                            .ack(BasicAckOptions::default())
                            .await
                        {
                            error!("Failed to ack message: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Error receiving message: {}", e);
                    }
                }
            }
            warn!("RabbitMQ consumer stopped");
        });

        Ok(())
    }

    /// Process an incoming event
    async fn process_event(
        db: &Arc<Database>,
        node: &Arc<RwLock<LspNode>>,
        delivery: &lapin::message::Delivery,
    ) -> anyhow::Result<()> {
        // Log raw payload details for debugging
        if delivery.data.is_empty() {
            error!("Received empty message from RabbitMQ");
            return Ok(());
        }
        
        debug!("Received event ({} bytes)", delivery.data.len());

        // Parse the protobuf event
        let event_envelope: EventEnvelope = match EventEnvelope::decode(&*delivery.data) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to decode protobuf event: {}. Data length: {} bytes. Raw hex: {}", 
                    e, delivery.data.len(), hex::encode(&delivery.data));
                return Ok(()); // Don't fail, just log and continue
            }
        };

        // Log what event type was received (for debugging all events)
        match &event_envelope.event {
            Some(Event::PaymentReceived(_)) => info!("RabbitMQ: Received PaymentReceived event"),
            Some(Event::PaymentSuccessful(_)) => info!("RabbitMQ: Received PaymentSuccessful event"),
            Some(Event::PaymentFailed(_)) => info!("RabbitMQ: Received PaymentFailed event"),
            Some(Event::PaymentForwarded(_)) => info!("RabbitMQ: Received PaymentForwarded event"),
            Some(Event::ChannelStateChange(_)) => info!("RabbitMQ: Received ChannelStateChange event"),
            Some(Event::ChannelClosed(_)) => info!("RabbitMQ: Received ChannelClosed event"),
            None => warn!("RabbitMQ: Received event envelope with no event data"),
        }

        // Handle the specific event type
        match event_envelope.event {
            Some(Event::PaymentReceived(payment_received)) => {
                if let Some(payment) = payment_received.payment {
                    let payment_hash = payment.id;  // Already hex-encoded
                    let amount_msat = payment.amount_msat.unwrap_or(0);
                    
                    info!(
                        "Received payment: hash={}, amount={} msat",
                        payment_hash, amount_msat
                    );

                    // DEPRECATED: Old channel request handling removed
                    // Only JIT receive requests are processed now
                    
                    // Also check if this payment is for a receive request
                    let receive_queries = crate::db::ReceiveRequestQueries::new(db);
                    
                    debug!("Looking for receive request with payment_hash: {}", payment_hash);
                    let receive_request = receive_queries.get_by_payment_hash(&payment_hash).await?;
                    
                    if let Some(receive_req) = receive_request {
                        debug!("Found receive request {} with status: {}", receive_req.id, receive_req.status);
                        
                        // Only process if still pending payment
                        if receive_req.status == "pending_payment" {
                            info!(
                                "Payment received for receive request: {}",
                                receive_req.id
                            );

                            // Update status to payment_received
                            receive_queries
                                .update_status(&receive_req.id, "payment_received", None)
                                .await?;

                            if receive_req.is_splice {
                                // Get channel details to find user_channel_id and counterparty_node_id
                                let channel_info = {
                                    let node = node.read().await;
                                    node.get_channel(&ChannelId(receive_req.channel_id.clone().unwrap_or_default())).await?
                                };
                                
                                if let Some(channel) = channel_info {
                                    // Use total channel capacity from request (includes inbound buffer)
                                    let splice_amount = receive_req.total_channel_capacity as u64;
                                    info!("Splicing in {} sats (requested: {} + buffer: {})", 
                                        splice_amount, receive_req.amount, splice_amount - (receive_req.amount as u64));
                                    
                                    // Execute the splice-in
                                    let splice_result = async {
                                        let node = node.read().await;
                                        node.splice_in(
                                            &channel.user_channel_id,
                                            &channel.counterparty_node_id,
                                            splice_amount,
                                        )
                                        .await
                                    }.await;
                                    
                                    match splice_result {
                                        Ok(()) => {
                                            info!("Splice-in initiated for receive request {}", receive_req.id);
                                            // Update status to splice_initiated
                                            receive_queries
                                                .update_status(&receive_req.id, "splice_initiated", Some(&channel.id.0))
                                                .await?;
                                        }
                                        Err(e) => {
                                            error!("Failed to initiate splice-in: {}", e);
                                            receive_queries
                                                .update_status_with_reason(&receive_req.id, "failed", Some(&channel.id.0), Some(&format!("Splice failed: {}", e)))
                                                .await?;
                                        }
                                    }
                                } else {
                                    error!("Channel {} not found for receive request {}", receive_req.channel_id.clone().unwrap_or_default(), receive_req.id);
                                    receive_queries
                                        .update_status_with_reason(&receive_req.id, "failed", receive_req.channel_id.as_deref(), Some("Channel not found"))
                                        .await?;
                                }
                            } else {
                                // Open new channel
                                let address = format!("{}:{}", receive_req.host, receive_req.port);
                                // Use total channel capacity from request (includes inbound buffer)
                                let channel_capacity = receive_req.total_channel_capacity as u64;
                                info!("Opening channel with {} sats capacity (requested: {} + buffer: {})", 
                                    channel_capacity, receive_req.amount, channel_capacity - (receive_req.amount as u64));
                                
                                let channel_open_result = async {
                                    let node = node.read().await;
                                    node.open_channel(
                                        &receive_req.node_id,
                                        &address,
                                        channel_capacity,
                                        0,
                                        false, // Private channel
                                    )
                                    .await
                                }.await;
                                
                                match channel_open_result {
                                    Ok(cid) => {
                                        info!("Channel creation initiated for receive request {}: user_channel_id={}", receive_req.id, cid);
                                        // Update status to channel_opening
                                        receive_queries
                                            .update_status(&receive_req.id, "channel_opening", Some(&cid.0))
                                            .await?;
                                    }
                                    Err(e) => {
                                        error!("Failed to initiate channel opening: {}", e);
                                        receive_queries
                                            .update_status_with_reason(&receive_req.id, "failed", None, Some(&format!("Channel open failed: {}", e)))
                                            .await?;
                                    }
                                }
                            }
                        } else {
                            debug!(
                                "Receive request {} is not in pending_payment status (current: {})",
                                receive_req.id, receive_req.status
                            );
                        }
                    } else {
                        debug!("No receive request found for payment_hash: {}", payment_hash);
                    }
                }
            }
            Some(Event::PaymentSuccessful(payment_successful)) => {
                if let Some(payment) = payment_successful.payment {
                    let payment_hash = payment.id;  // Already hex-encoded
                    info!("Payment successful: hash={}", payment_hash);
                }
            }
            Some(Event::PaymentFailed(payment_failed)) => {
                if let Some(payment) = payment_failed.payment {
                    let payment_hash = payment.id;  // Already hex-encoded
                    info!("Payment failed: hash={}", payment_hash);
                }
            }
            Some(Event::PaymentForwarded(payment_forwarded)) => {
                if let Some(forwarded) = payment_forwarded.forwarded_payment {
                    info!(
                        "Payment forwarded: prev_channel_id={}, next_channel_id={}, fee_earned_msat={:?}",
                        forwarded.prev_channel_id,
                        forwarded.next_channel_id,
                        forwarded.total_fee_earned_msat
                    );
                }
            }
            Some(Event::ChannelStateChange(channel_state_change)) => {
                if let Some(channel) = channel_state_change.channel {
                    let user_channel_id = &channel.user_channel_id;
                    let state = channel_state_change.state;
                    
                    info!(
                        "Channel state change: user_channel_id={}, state={}",
                        user_channel_id, state
                    );
                    
                    // DEPRECATED: Old channel request and splice handling removed
                    // Check if this channel state change is for a pending receive request
                    let receive_queries = ReceiveRequestQueries::new(db);
                    let receive_queries = crate::db::ReceiveRequestQueries::new(db);
                    
                    // Find receive requests by channel_id that are in channel_opening or splice_initiated status
                    match receive_queries.get_by_channel_id_and_status(&channel.channel_id, "channel_opening").await {
                        Ok(receive_requests) => {
                            for receive_req in receive_requests {
                                match state {
                                    1 => { // READY = 1
                                        receive_queries
                                            .update_status(&receive_req.id, "completed", Some(&channel.channel_id))
                                            .await?;
                                        info!(
                                            "Channel ready for receive request {}: channel_id={}",
                                            receive_req.id, channel.channel_id
                                        );
                                    }
                                    2 => { // CLOSED = 2
                                        receive_queries
                                            .update_status_with_reason(&receive_req.id, "failed", Some(&channel.channel_id), Some("Channel closed"))
                                            .await?;
                                        error!(
                                            "Channel {} closed for receive request {}",
                                            channel.channel_id, receive_req.id
                                        );
                                    }
                                    _ => {
                                        debug!("Channel {} state changed to {} for receive request {}", channel.channel_id, state, receive_req.id);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to get receive requests for channel: {}", e);
                        }
                    }
                    
                    // Also check splice_initiated receive requests
                    match receive_queries.get_by_channel_id_and_status(&channel.channel_id, "splice_initiated").await {
                        Ok(receive_requests) => {
                            for receive_req in receive_requests {
                                match state {
                                    1 => { // READY = 1
                                        // Verify the splice was successful by checking capacity
                                        let new_capacity = channel.channel_value_sats;
                                        let expected_capacity = receive_req.amount as u64;
                                        let capacity_increased = new_capacity >= expected_capacity.saturating_sub(expected_capacity / 100);
                                        
                                        if capacity_increased {
                                            receive_queries
                                                .update_status(&receive_req.id, "completed", Some(&channel.channel_id))
                                                .await?;
                                            info!(
                                                "Splice completed for receive request {}: channel_id={}, new_capacity={}",
                                                receive_req.id, channel.channel_id, new_capacity
                                            );
                                        } else {
                                            receive_queries
                                                .update_status(&receive_req.id, "splice_verification_failed", Some(&channel.channel_id))
                                                .await?;
                                            warn!(
                                                "Splice verification failed for receive request {}: channel_id={}, new_capacity={}, expected={}",
                                                receive_req.id, channel.channel_id, new_capacity, expected_capacity
                                            );
                                        }
                                    }
                                    2 => { // CLOSED = 2
                                        receive_queries
                                            .update_status_with_reason(&receive_req.id, "failed", Some(&channel.channel_id), Some("Channel closed"))
                                            .await?;
                                        error!(
                                            "Channel {} closed during splice for receive request {}",
                                            channel.channel_id, receive_req.id
                                        );
                                    }
                                    _ => {
                                        debug!("Channel {} state changed to {} during splice for receive request {}", channel.channel_id, state, receive_req.id);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to get receive requests for channel: {}", e);
                        }
                    }
                }
            }
            Some(Event::ChannelClosed(channel_closed)) => {
                let user_channel_id = &channel_closed.user_channel_id;
                let is_open_failure = channel_closed.is_open_failure;
                let is_force_close = channel_closed.is_force_close;
                let reason_description = &channel_closed.reason_description;
                
                info!(
                    "Channel closed: user_channel_id={}, is_open_failure={}, is_force_close={}, reason={}",
                    user_channel_id, is_open_failure, is_force_close, reason_description
                );
                
                // DEPRECATED: Old channel request handling removed
            }
            None => {
                debug!("Received event envelope with no event data");
            }
        }

        Ok(())
    }
}

use futures::StreamExt;
