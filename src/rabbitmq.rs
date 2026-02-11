//! RabbitMQ event consumer for ldk-server events
//!
//! This module consumes events from ldk-server's RabbitMQ exchange
//! and triggers appropriate actions (like opening channels when payments are received).

use crate::{
    config::RabbitMqConfig,
    db::{ChannelRequestQueries, Database},
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

                    // Check if this payment is for a channel request
                    let queries = ChannelRequestQueries::new(db);
                    
                    // Find request by payment hash
                    debug!("Looking for channel request with payment_hash: {}", payment_hash);
                    let request = queries.get_by_payment_hash(&payment_hash).await?;
                    
                    if let Some(request) = request {
                        debug!("Found channel request {} with status: {}", request.id, request.status);
                        
                        // Only process if still pending payment
                        if request.status == "pending_payment" {
                            info!(
                                "Payment received for channel request: {}",
                                request.id
                            );

                            // Update status to payment_received
                            queries
                                .update_status(&request.id, "payment_received", None)
                                .await?;

                            // Open the channel
                            let address = format!("{}:{}", request.host, request.port);
                            let channel_open_result = async {
                                let node = node.read().await;
                                node.open_channel(
                                    &request.node_id,
                                    &address,
                                    request.capacity as u64,
                                    0,
                                    false, // Private channel (not announced)
                                )
                                .await
                            }.await;
                            
                            match channel_open_result {
                                Ok(cid) => {
                                    info!("Channel creation initiated for request {}: user_channel_id={}", request.id, cid);
                                    // Update status to channel_opening (initiated but not yet confirmed)
                                    // ChannelStateChange event will update this to channel_opened when ready
                                    queries
                                        .update_status(&request.id, "channel_opening", Some(&cid.0))
                                        .await?;
                                }
                                Err(e) => {
                                    error!("Failed to initiate channel opening: {}", e);
                                    queries
                                        .update_status(
                                            &request.id,
                                            "channel_open_failed",
                                            None,
                                        )
                                        .await?;
                                }
                            }
                        } else {
                            debug!(
                                "Request {} is not in pending_payment status (current: {})",
                                request.id, request.status
                            );
                        }
                    } else {
                        debug!("No channel request found for payment_hash: {}", payment_hash);
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
                    
                    // Check if this channel belongs to a pending channel request
                    let queries = ChannelRequestQueries::new(db);
                    
                    // Find request by channel_id (stored as user_channel_id)
                    match queries.get_by_channel_id(user_channel_id).await {
                        Ok(Some(request)) => {
                            if request.status == "channel_opening" {
                                match state {
                                    1 => { // READY = 1
                                        queries
                                            .update_status(&request.id, "channel_opened", Some(user_channel_id))
                                            .await?;
                                        info!(
                                            "Channel {} confirmed opened for request {}",
                                            user_channel_id, request.id
                                        );
                                    }
                                    2 => { // CLOSED = 2
                                        queries
                                            .update_status_with_reason(&request.id, "channel_open_failed", Some(user_channel_id), Some("Channel closed before ready"))
                                            .await?;
                                        error!(
                                            "Channel {} failed to open (closed) for request {}",
                                            user_channel_id, request.id
                                        );
                                    }
                                    _ => {
                                        debug!("Channel {} in pending state", user_channel_id);
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            debug!("No channel request found for user_channel_id: {}", user_channel_id);
                        }
                        Err(e) => {
                            error!("Failed to get channel request: {}", e);
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
                
                // Check if this channel belongs to a pending channel request
                let queries = ChannelRequestQueries::new(db);
                
                match queries.get_by_channel_id(user_channel_id).await {
                    Ok(Some(request)) => {
                        if is_open_failure {
                            // Channel failed to open (never became ready)
                            queries
                                .update_status_with_reason(&request.id, "channel_open_failed", Some(user_channel_id), Some(reason_description))
                                .await?;
                            error!(
                                "Channel {} failed to open for request {}: {}",
                                user_channel_id, request.id, reason_description
                            );
                        } else {
                            // Channel was closed after being ready
                            let close_status = if is_force_close { "force_closed" } else { "closed" };
                            queries
                                .update_status_with_reason(&request.id, close_status, Some(user_channel_id), Some(reason_description))
                                .await?;
                            info!(
                                "Channel {} {} for request {}: {}",
                                user_channel_id, close_status, request.id, reason_description
                            );
                        }
                    }
                    Ok(None) => {
                        debug!("No channel request found for user_channel_id: {}", user_channel_id);
                    }
                    Err(e) => {
                        error!("Failed to get channel request: {}", e);
                    }
                }
            }
            None => {
                debug!("Received event envelope with no event data");
            }
        }

        Ok(())
    }
}

use futures::StreamExt;
