//! Configuration management for LDK-LSP
//!
//! Configuration is loaded from TOML files and environment variables.
//!
//! # Example Configuration File
//!
//! ```toml
//! [node]
//! alias = "My LSP"
//! data_dir = "/var/lib/ldk-lsp"
//! network = "mainnet"
//!
//! [ldk_server]
//! host = "127.0.0.1"
//! port = 3009
//! tls_certificate = "/path/to/cert.pem"
//!
//! [lsp]
//! channel_open_base_fee = 10000
//! channel_open_fee_ppm = 10000
//! min_channel_size = 100000
//! max_channel_size = 100000000
//! enable_zeroconf = true
//! enable_splicing = true
//!
//! [api]
//! bind_address = "0.0.0.0:8080"
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Node identity configuration
    #[serde(default)]
    pub node: NodeConfig,

    /// LDK Server connection configuration
    #[serde(rename = "ldk_server", default)]
    pub ldk_server: LdkServerConfig,

    /// LSP service configuration
    #[serde(default)]
    pub lsp: LspConfig,

    /// API server configuration
    #[serde(default)]
    pub api: ApiConfig,

    /// Database configuration
    #[serde(default)]
    pub database: DatabaseConfig,

    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            node: NodeConfig::default(),
            ldk_server: LdkServerConfig::default(),
            lsp: LspConfig::default(),
            api: ApiConfig::default(),
            database: DatabaseConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

/// Lightning node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node alias (display name)
    #[serde(default = "default_node_alias")]
    pub alias: String,

    /// Data directory for storing node state
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Network to run on (mainnet, testnet, signet, regtest)
    #[serde(default = "default_network")]
    pub network: String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            alias: default_node_alias(),
            data_dir: default_data_dir(),
            network: default_network(),
        }
    }
}

fn default_node_alias() -> String {
    "LDK-LSP".to_string()
}

fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .map(|d| d.join("ldk-lsp"))
        .unwrap_or_else(|| PathBuf::from("./data"))
}

fn default_network() -> String {
    "regtest".to_string()
}

/// LDK Server connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdkServerConfig {
    /// LDK Server host address
    #[serde(default = "default_ldk_server_host")]
    pub host: String,

    /// LDK Server port
    #[serde(default = "default_ldk_server_port")]
    pub port: u16,

    /// Path to TLS certificate (if using TLS)
    pub tls_certificate: Option<PathBuf>,

    /// Authentication token (if required)
    pub auth_token: Option<String>,

    /// Path to API key file (if not using auth_token directly)
    /// ldk-server generates this on first startup
    pub api_key_file: Option<PathBuf>,

    /// Connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub timeout_seconds: u64,

    /// RabbitMQ configuration for async events
    pub rabbitmq: Option<RabbitMqConfig>,
}

/// RabbitMQ configuration for receiving ldk-server events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RabbitMqConfig {
    /// RabbitMQ connection string (e.g., "amqp://guest:guest@localhost:5672/%2F")
    /// Note: The vhost "/" must be URL-encoded as "%2F"
    pub connection_string: String,

    /// Exchange name to listen on (should match ldk-server's exchange_name)
    pub exchange_name: String,

    /// Queue name for this LSP instance (optional, will be auto-generated if not set)
    pub queue_name: Option<String>,
}

impl Default for LdkServerConfig {
    fn default() -> Self {
        Self {
            host: default_ldk_server_host(),
            port: default_ldk_server_port(),
            tls_certificate: None,
            auth_token: None,
            api_key_file: None,
            timeout_seconds: default_connection_timeout(),
            rabbitmq: None,
        }
    }
}

fn default_ldk_server_host() -> String {
    "127.0.0.1".to_string()
}

fn default_ldk_server_port() -> u16 {
    3009 // Default ldk-server port
}

fn default_connection_timeout() -> u64 {
    30
}

/// LSP service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspConfig {
    /// Base fee for opening a channel (satoshis)
    #[serde(default = "default_channel_open_base_fee")]
    pub channel_open_base_fee: u64,

    /// Fee rate for channel capacity (parts per million)
    #[serde(default = "default_channel_open_fee_ppm")]
    pub channel_open_fee_ppm: u64,

    /// Minimum channel size (satoshis)
    #[serde(default = "default_min_channel_size")]
    pub min_channel_size: u64,

    /// Maximum channel size (satoshis)
    #[serde(default = "default_max_channel_size")]
    pub max_channel_size: u64,

    /// Default channel confirmation target (blocks)
    #[serde(default = "default_confirmation_target")]
    pub confirmation_target: u32,

    /// Enable zeroconf channels
    #[serde(default = "default_true")]
    pub enable_zeroconf: bool,

    /// Enable splicing
    #[serde(default = "default_true")]
    pub enable_splicing: bool,

    /// Zeroconf minimum channel size (satoshis)
    #[serde(default = "default_zeroconf_min_size")]
    pub zeroconf_min_size: u64,

    /// Maximum number of pending zeroconf channels per peer
    #[serde(default = "default_max_pending_zeroconf")]
    pub max_pending_zeroconf: u32,
}

impl Default for LspConfig {
    fn default() -> Self {
        Self {
            channel_open_base_fee: default_channel_open_base_fee(),
            channel_open_fee_ppm: default_channel_open_fee_ppm(),
            min_channel_size: default_min_channel_size(),
            max_channel_size: default_max_channel_size(),
            confirmation_target: default_confirmation_target(),
            enable_zeroconf: true,
            enable_splicing: true,
            zeroconf_min_size: default_zeroconf_min_size(),
            max_pending_zeroconf: default_max_pending_zeroconf(),
        }
    }
}

fn default_channel_open_base_fee() -> u64 {
    10_000 // 10k sats base fee
}

fn default_channel_open_fee_ppm() -> u64 {
    10_000 // 1% fee
}

fn default_min_channel_size() -> u64 {
    100_000 // 100k sats minimum
}

fn default_max_channel_size() -> u64 {
    100_000_000 // 1 BTC maximum
}

fn default_confirmation_target() -> u32 {
    6
}

fn default_zeroconf_min_size() -> u64 {
    50_000 // 50k sats minimum for zeroconf
}

fn default_max_pending_zeroconf() -> u32 {
    5 // Max 5 pending zeroconf channels per peer
}

fn default_true() -> bool {
    true
}

/// API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Address to bind the API server to
    #[serde(default = "default_api_bind")]
    pub bind_address: String,

    /// API request timeout in seconds
    #[serde(default = "default_api_timeout")]
    pub timeout_seconds: u64,

    /// Enable CORS
    #[serde(default = "default_true")]
    pub enable_cors: bool,

    /// API rate limiting (requests per minute)
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_minute: u32,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind_address: default_api_bind(),
            timeout_seconds: default_api_timeout(),
            enable_cors: true,
            rate_limit_per_minute: default_rate_limit(),
        }
    }
}

fn default_api_bind() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_api_timeout() -> u64 {
    30
}

fn default_rate_limit() -> u32 {
    60 // 60 requests per minute
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database URL or path
    #[serde(default = "default_database_url")]
    pub url: String,

    /// Maximum database connections
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: default_database_url(),
            max_connections: default_max_connections(),
        }
    }
}

fn default_database_url() -> String {
    "sqlite:ldk-lsp.db".to_string()
}

impl Config {
    /// Resolve the database URL, making it relative to data_dir if needed
    pub fn resolve_database_url(&self) -> String {
        let url = &self.database.url;

        // If it's already an absolute path or :memory:, use as-is
        if url.starts_with("sqlite:/") || url == "sqlite::memory:" {
            return url.clone();
        }

        // Extract the path part
        let path = if url.starts_with("sqlite:") {
            url.strip_prefix("sqlite:").unwrap_or(url)
        } else {
            url
        };

        // If it's already absolute, use as-is
        if std::path::Path::new(path).is_absolute() {
            return url.clone();
        }

        // Make it relative to data_dir
        let db_path = self.node.data_dir.join(path);
        format!("sqlite:{}", db_path.display())
    }
}

fn default_max_connections() -> u32 {
    10
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Log format (pretty, compact, json)
    #[serde(default = "default_log_format")]
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "pretty".to_string()
}

impl Config {
    /// Get the API bind address
    pub fn api_bind_address(&self) -> String {
        self.api.bind_address.clone()
    }

    /// Get the LDK Server connection address
    pub fn ldk_server_address(&self) -> String {
        format!("{}:{}", self.ldk_server.host, self.ldk_server.port)
    }

    /// Check if running on mainnet
    pub fn is_mainnet(&self) -> bool {
        self.node.network == "mainnet"
    }

    /// Calculate the total fee for opening a channel
    pub fn calculate_channel_fee(&self, capacity: u64) -> u64 {
        let ppm_fee = capacity.saturating_mul(self.lsp.channel_open_fee_ppm) / 1_000_000;
        self.lsp.channel_open_base_fee.saturating_add(ppm_fee)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate channel size constraints
        if self.lsp.min_channel_size >= self.lsp.max_channel_size {
            return Err("Minimum channel size must be less than maximum channel size".to_string());
        }

        // Validate fee rates
        if self.lsp.channel_open_fee_ppm > 1_000_000 {
            return Err("Fee rate cannot exceed 100% (1,000,000 ppm)".to_string());
        }

        // Validate network
        let valid_networks = ["mainnet", "testnet", "signet", "regtest"];
        if !valid_networks.contains(&self.node.network.as_str()) {
            return Err(format!(
                "Invalid network: {}. Must be one of: {:?}",
                self.node.network, valid_networks
            ));
        }

        // Validate LDK Server port
        if self.ldk_server.port == 0 {
            return Err("LDK Server port cannot be 0".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_channel_fee() {
        let config = Config::default();
        // Base fee: 10000, PPM: 10000 (1%)
        // For 1M sats: 10000 + (1M * 0.01) = 10000 + 10000 = 20000
        assert_eq!(config.calculate_channel_fee(1_000_000), 20_000);
    }

    #[test]
    fn test_validate_config() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());

        // Invalid: min >= max
        config.lsp.min_channel_size = 100_000_000;
        assert!(config.validate().is_err());

        // Reset and test invalid fee
        config.lsp.min_channel_size = 100_000;
        config.lsp.channel_open_fee_ppm = 2_000_000;
        assert!(config.validate().is_err());
    }
}
