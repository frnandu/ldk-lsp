//! LDK-LSP: A flexible Lightning Service Provider built on LDK Server
//!
//! This crate provides a Lightning Service Provider (LSP) that extends
//! [LDK Server](https://github.com/lightningdevkit/ldk-server) with LSP-specific features:
//!
//! - **Zero-confirmation (zeroconf) channel opens**: Allow clients to receive payments
//!   immediately without waiting for on-chain confirmations
//! - **Channel splicing**: Increase inbound capacity by splicing existing channels
//! - **HTTP API**: RESTful API for channel purchases and management
//! - **RabbitMQ Events**: Async event consumption for real-time payment notifications
//!
//! # Architecture
//!
//! The LSP operates as a layer on top of LDK Server:
//!
//! 1. Uses `ldk-server-client` to communicate with the Lightning node
//! 2. Provides an HTTP API for external services to purchase channels
//! 3. Manages channel lifecycle including zeroconf handling
//! 4. Consumes RabbitMQ events from ldk-server for async payment notifications
//!
#![warn(missing_docs)]

pub mod api;
pub mod config;
pub mod db;
pub mod lsp;
pub mod node;
pub mod rabbitmq;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

pub use config::Config;
use db::Database;
use node::LspNode;

/// The main LSP application state
#[derive(Clone)]
pub struct LspApp {
    /// Application configuration
    pub config: Arc<Config>,
    /// The LSP node (wrapper around ldk-server-client)
    pub node: Arc<RwLock<LspNode>>,
    /// Database connection
    pub db: Arc<Database>,
    /// LSP service for channel management
    pub lsp_service: Arc<lsp::LspService>,
}

impl LspApp {
    /// Create a new LSP application instance
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing LDK-LSP application...");

        let config = Arc::new(config);

        // Initialize the database
        let db_url = config.resolve_database_url();
        info!("Connecting to database at: {}", db_url);
        let db = Database::connect(&db_url).await?;
        let db = Arc::new(db);
        info!("Database connected successfully");

        // Initialize the LSP node (connects to ldk-server)
        let node = LspNode::new(config.clone()).await?;
        let node = Arc::new(RwLock::new(node));
        
        // Create the LSP service
        let lsp_service = Arc::new(lsp::LspService::new(config.clone(), db.clone()));

        info!("LDK-LSP application initialized successfully");

        Ok(Self { config, node, db, lsp_service })
    }

    /// Start the LSP application
    pub async fn run(&self) -> Result<()> {
        self.run_with_shutdown(tokio::sync::oneshot::channel().1).await
    }

    /// Start the LSP application with shutdown signal
    pub async fn run_with_shutdown(&self, shutdown_rx: tokio::sync::oneshot::Receiver<()>) -> Result<()> {
        info!("Starting LDK-LSP application...");

        // Start the LSP node
        {
            let mut node = self.node.write().await;
            node.start().await?;
        }
        info!("LDK Server node started successfully");
        
        // Initialize LSP service (runs reconciliations)
        info!("Initializing LSP service and running reconciliations...");
        if let Err(e) = self.lsp_service.init(self.node.clone()).await {
            error!("Failed to initialize LSP service: {}", e);
            return Err(e.into());
        }
        info!("LSP service initialized successfully");

        // Start RabbitMQ consumer if configured
        if let Some(rabbitmq_config) = &self.config.ldk_server.rabbitmq {
            info!("Starting RabbitMQ event consumer...");
            let mut consumer = rabbitmq::RabbitMqConsumer::new(
                rabbitmq_config.clone(),
                self.db.clone(),
                self.node.clone(),
            );
            if let Err(e) = consumer.start().await {
                error!("Failed to start RabbitMQ consumer: {}", e);
                // Don't fail the entire startup, just warn
            } else {
                info!("RabbitMQ event consumer started successfully");
            }
        } else {
            info!("RabbitMQ not configured, using polling mode for payment detection");
        }

        // Start the HTTP API server with shutdown handler
        let api_handle = tokio::spawn({
            let app = self.clone();
            async move {
                if let Err(e) = api::serve_with_shutdown(app, shutdown_rx).await {
                    warn!("API server error: {}", e);
                }
            }
        });

        info!(
            "LDK-LSP application running. API available at http://{}",
            self.config.api_bind_address()
        );

        // Wait for the API server
        api_handle.await?;

        Ok(())
    }

    /// Shutdown the LSP application gracefully
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down LDK-LSP application...");

        let mut node = self.node.write().await;
        node.stop().await?;

        self.db.close().await;

        info!("LDK-LSP application shutdown complete");
        Ok(())
    }
}

/// Error types for the LSP application
#[derive(thiserror::Error, Debug)]
pub enum LspError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Lightning node error
    #[error("Lightning node error: {0}")]
    Node(String),

    /// gRPC communication error
    #[error("gRPC error: {0}")]
    Grpc(String),

    /// Database error
    #[error("Database error: {0}")]
    Database(String),

    /// API error
    #[error("API error: {0}")]
    Api(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Channel operation error
    #[error("Channel error: {0}")]
    Channel(String),
}

/// Result type alias for LSP operations
pub type LspResult<T> = std::result::Result<T, LspError>;
