//! Database module for LDK-LSP
//!
//! This module handles persistent storage for:
//! - Channel requests
//! - Payment records
//! - Splice operations
//! - User/peer data

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

        // Create tables if they don't exist
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS channel_requests (
                id TEXT PRIMARY KEY,
                node_id TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                capacity INTEGER NOT NULL,
                fee INTEGER NOT NULL,
                require_zeroconf BOOLEAN NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                channel_id TEXT,
                payment_hash TEXT,
                failure_reason TEXT,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
            [],
        )?;

        // Migration: Add failure_reason column if it doesn't exist (for existing databases)
        let column_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('channel_requests') WHERE name = 'failure_reason'",
            [],
            |row| row.get(0),
        ).unwrap_or(0) > 0;
        
        if !column_exists {
            debug!("Adding failure_reason column to channel_requests table...");
            conn.execute(
                "ALTER TABLE channel_requests ADD COLUMN failure_reason TEXT",
                [],
            )?;
        }

        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS payments (
                id TEXT PRIMARY KEY,
                payment_hash TEXT UNIQUE NOT NULL,
                request_id TEXT,
                amount_msat INTEGER NOT NULL,
                status TEXT NOT NULL,
                preimage TEXT,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                settled_at DATETIME,
                FOREIGN KEY (request_id) REFERENCES channel_requests(id)
            )
            "#,
            [],
        )?;

        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS splice_requests (
                id TEXT PRIMARY KEY,
                channel_id TEXT NOT NULL,
                operation_type TEXT NOT NULL,
                amount INTEGER NOT NULL,
                address TEXT,
                status TEXT NOT NULL,
                txid TEXT,
                payment_hash TEXT,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
            [],
        )?;

        // Migration: Add payment_hash column if it doesn't exist (for existing databases)
        let splice_column_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('splice_requests') WHERE name = 'payment_hash'",
            [],
            |row| row.get(0),
        ).unwrap_or(0) > 0;

        if !splice_column_exists {
            debug!("Adding payment_hash column to splice_requests table...");
            conn.execute(
                "ALTER TABLE splice_requests ADD COLUMN payment_hash TEXT",
                [],
            )?;
        }

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

        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_channel_requests_node_id ON channel_requests(node_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_channel_requests_status ON channel_requests(status)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_payments_hash ON payments(payment_hash)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_splice_channel_id ON splice_requests(channel_id)",
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
