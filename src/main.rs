use anyhow::Result;
use std::path::PathBuf;
use tracing::{error, info};

use ldk_lsp::{config::Config, LspApp};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(true)
        .with_thread_ids(true)
        .init();

    info!("Starting LDK-LSP node...");

    // Load configuration
    let config = load_config().await?;

    // Validate configuration
    if let Err(e) = config.validate() {
        error!("Configuration validation failed: {}", e);
        std::process::exit(1);
    }

    // Create and run the LSP application
    let app = LspApp::new(config).await?;

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Handle shutdown signals
    let app_clone = app.clone();
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("Received shutdown signal, initiating graceful shutdown...");
                let _ = shutdown_tx.send(());
                if let Err(e) = app_clone.shutdown().await {
                    error!("Error during shutdown: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to listen for shutdown signal: {}", e);
            }
        }
    });

    // Run the application with shutdown handler
    app.run_with_shutdown(shutdown_rx).await?;

    info!("LDK-LSP server stopped gracefully");
    Ok(())
}

/// Load configuration from file or use defaults
async fn load_config() -> Result<Config> {
    // Look for config in standard locations
    let config_paths = vec![
        PathBuf::from("./ldk-lsp.toml"),
        PathBuf::from("/etc/ldk-lsp/ldk-lsp.toml"),
        dirs::config_dir()
            .map(|d| d.join("ldk-lsp/ldk-lsp.toml"))
            .unwrap_or_default(),
    ];

    for path in config_paths {
        if path.exists() {
            info!("Loading configuration from: {}", path.display());
            let content = tokio::fs::read_to_string(&path).await?;
            let config: Config = toml::from_str(&content)?;
            return Ok(config);
        }
    }

    info!("No configuration file found, using defaults");
    Ok(Config::default())
}