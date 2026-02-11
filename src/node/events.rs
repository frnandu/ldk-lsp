//! Event handling for LDK Server events

use crate::LspResult;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Event handler for LDK Server events
pub struct EventHandler {
    /// Sender for internal event processing
    event_sender: Option<mpsc::Sender<NodeEvent>>,
    /// Broadcast channel for event subscriptions
    broadcast_tx: Option<broadcast::Sender<NodeEvent>>,
    /// Handle to the event processing task
    task_handle: Option<tokio::task::JoinHandle<()>>,
    /// Running state
    running: Arc<RwLock<bool>>,
}

impl EventHandler {
    /// Create a new event handler
    pub fn new() -> Self {
        Self {
            event_sender: None,
            broadcast_tx: None,
            task_handle: None,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start the event handler
    pub async fn start(&mut self) -> LspResult<()> {
        info!("Starting event handler...");

        let (event_tx, mut event_rx) = mpsc::channel::<NodeEvent>(100);
        let (broadcast_tx, _) = broadcast::channel::<NodeEvent>(100);

        self.event_sender = Some(event_tx);
        self.broadcast_tx = Some(broadcast_tx.clone());

        let running = self.running.clone();
        *running.write().await = true;

        let handle = tokio::spawn(async move {
            info!("Event processing task started");

            while *running.read().await {
                match tokio::time::timeout(
                    tokio::time::Duration::from_millis(100),
                    event_rx.recv()
                ).await {
                    Ok(Some(event)) => {
                        Self::process_event(event.clone());
                        if let Err(e) = broadcast_tx.send(event) {
                            warn!("Failed to broadcast event: {}", e);
                        }
                    }
                    Ok(None) => {
                        info!("Event channel closed");
                        break;
                    }
                    Err(_) => continue,
                }
            }

            info!("Event processing task stopped");
        });

        self.task_handle = Some(handle);
        info!("Event handler started");
        Ok(())
    }

    /// Stop the event handler
    pub async fn stop(&mut self) -> LspResult<()> {
        info!("Stopping event handler...");
        *self.running.write().await = false;
        self.event_sender = None;
        self.broadcast_tx = None;

        if let Some(handle) = self.task_handle.take() {
            match tokio::time::timeout(tokio::time::Duration::from_secs(5), handle).await {
                Ok(Ok(())) => info!("Event handler stopped cleanly"),
                Ok(Err(e)) => warn!("Event handler task panicked: {}", e),
                Err(_) => warn!("Event handler stop timed out"),
            }
        }

        Ok(())
    }

    /// Process a single event
    fn process_event(event: NodeEvent) {
        match event {
            NodeEvent::ChannelOpened { channel_id, counterparty, capacity_sat } => {
                info!("Channel opened: {} with {} (capacity: {} sat)",
                    channel_id, counterparty, capacity_sat);
            }
            NodeEvent::ChannelClosed { channel_id, reason } => {
                info!("Channel closed: {} (reason: {:?})", channel_id, reason);
            }
            NodeEvent::PaymentReceived { payment_hash, amount_msat } => {
                info!("Payment received: {} (amount: {} msat)",
                    payment_hash, amount_msat);
            }
            NodeEvent::PaymentSent { payment_hash, payment_id } => {
                info!("Payment sent: {} (id: {})", payment_hash, payment_id);
            }
            NodeEvent::PaymentFailed { payment_id, reason } => {
                warn!("Payment failed: {} (reason: {:?})", payment_id, reason);
            }
            NodeEvent::PeerConnected { node_id, address } => {
                debug!("Peer connected: {} at {}", node_id, address);
            }
            NodeEvent::PeerDisconnected { node_id } => {
                debug!("Peer disconnected: {}", node_id);
            }
            NodeEvent::ChannelReady { channel_id } => {
                info!("Channel ready: {}", channel_id);
            }
        }
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> Option<broadcast::Receiver<NodeEvent>> {
        self.broadcast_tx.as_ref().map(|tx| tx.subscribe())
    }

    /// Emit an event
    pub async fn emit(&self, event: NodeEvent) -> LspResult<()> {
        if let Some(sender) = &self.event_sender {
            sender.send(event).await
                .map_err(|e| crate::LspError::Node(format!("Failed to emit event: {}", e)))?;
        }
        Ok(())
    }

    /// Check if the handler is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Node events that can occur
#[derive(Debug, Clone)]
pub enum NodeEvent {
    /// A channel was opened
    ChannelOpened {
        channel_id: String,
        counterparty: String,
        capacity_sat: u64,
    },
    /// A channel was closed
    ChannelClosed {
        channel_id: String,
        reason: Option<String>,
    },
    /// A payment was received
    PaymentReceived {
        payment_hash: String,
        amount_msat: u64,
    },
    /// A payment was sent
    PaymentSent {
        payment_hash: String,
        payment_id: String,
    },
    /// A payment failed
    PaymentFailed {
        payment_id: String,
        reason: Option<String>,
    },
    /// A peer connected
    PeerConnected {
        node_id: String,
        address: String,
    },
    /// A peer disconnected
    PeerDisconnected {
        node_id: String,
    },
    /// A channel is ready for use
    ChannelReady {
        channel_id: String,
    },
}