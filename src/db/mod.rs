//! Database module for LDK-LSP
//!
//! This module handles persistent storage for:
//! - JIT Receive requests (inbound liquidity)
//! - Peer tracking

use rusqlite::{Connection, Result as SqliteResult};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

mod models;
mod queries;

pub use models::*;
pub use queries::*;

/// Database connection pool
#[derive(Clone)]
pub struct Database {
    /// SQLite connection (wrapped in Arc<Mutex> for thread safety)
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Connect to the database
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        info!("Connecting to database at {}", database_url);

        // Parse the database URL
        let path = if database_url.starts_with("sqlite:") {
            database_url.strip_prefix("sqlite:").unwrap_or(database_url)
        } else {
            database_url
        };

        // Ensure the directory exists for file-based databases
        if path != ":memory:" {
            if let Some(parent) = Path::new(path).parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        // Open the connection
        let conn = Connection::open(path)?;

        // Run migrations
        Self::run_migrations(&conn)?;

        info!("Database connected successfully");

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Run database migrations
    fn run_migrations(conn: &Connection) -> anyhow::Result<()> {
        debug!("Running database migrations...");

        // Create peers table for tracking node information
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS peers (
                node_id TEXT PRIMARY KEY,
                alias TEXT,
                first_seen DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                last_connected DATETIME,
                trust_score TEXT DEFAULT 'new',
                total_channels INTEGER DEFAULT 0,
                zeroconf_violations INTEGER DEFAULT 0
            )
            "#,
            [],
        )?;

        // Check if receive_requests table exists and needs to be recreated
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='receive_requests'",
            [],
            |row| row.get(0),
        ).unwrap_or(0) > 0;

        if table_exists {
            // Check if required columns exist
            let required_columns = ["total_channel_capacity", "fee_rate", "user_invoice", "reserve_amount"];
            for col in &required_columns {
                let column_exists: bool = conn.query_row(
                    &format!("SELECT COUNT(*) FROM pragma_table_info('receive_requests') WHERE name='{}'", col),
                    [],
                    |row| row.get(0),
                ).unwrap_or(0) > 0;

                if !column_exists {
                    info!("Dropping old receive_requests table to add {} column...", col);
                    conn.execute("DROP TABLE receive_requests", [])?;
                    break;
                }
            }
        }

        // Create receive_requests table for JIT liquidity
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS receive_requests (
                id TEXT PRIMARY KEY,
                node_id TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                amount INTEGER NOT NULL,
                fee_base INTEGER NOT NULL,
                fee_ppm INTEGER NOT NULL,
                fee_onchain INTEGER NOT NULL,
                fee_total INTEGER NOT NULL,
                reserve_amount INTEGER NOT NULL DEFAULT 0,
                fee_rate INTEGER NOT NULL,
                total_invoice_amount INTEGER NOT NULL,
                total_channel_capacity INTEGER NOT NULL,
                is_splice BOOLEAN NOT NULL DEFAULT 0,
                channel_id TEXT,
                status TEXT NOT NULL,
                payment_hash TEXT,
                user_invoice TEXT,
                user_payment_hash TEXT,
                user_invoice_paid BOOLEAN NOT NULL DEFAULT 0,
                failure_reason TEXT,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
            [],
        )?;

        // Create indexes for receive_requests
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_receive_node_id ON receive_requests(node_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_receive_status ON receive_requests(status)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_receive_payment_hash ON receive_requests(payment_hash)",
            [],
        )?;

        debug!("Database migrations completed");
        Ok(())
    }

    /// Get the database connection
    pub fn conn(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    /// Close the database connection
    pub async fn close(&self) {
        info!("Closing database connection...");
        // The connection will be closed when the Arc is dropped
        info!("Database connection closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_connect() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let conn_lock = db.conn();
        let conn = conn_lock.lock().await;
        let count: i64 = conn
            .query_row("SELECT 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
