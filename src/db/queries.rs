//! Database queries

use super::{ChannelRequestModel, Database, PaymentModel, PeerModel, SpliceRequestModel};
use anyhow::Result;
use rusqlite::OptionalExtension;
use tracing::info;

/// Channel request queries
pub struct ChannelRequestQueries<'a> {
    db: &'a Database,
}

impl<'a> ChannelRequestQueries<'a> {
    /// Create a new query instance
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Insert a new channel request
    pub async fn insert(&self, request: &ChannelRequestModel) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO channel_requests (id, node_id, host, port, capacity, fee, require_zeroconf, status, channel_id, payment_hash, failure_reason, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            rusqlite::params![
                &request.id,
                &request.node_id,
                &request.host,
                request.port,
                request.capacity,
                request.fee,
                request.require_zeroconf,
                &request.status,
                request.channel_id.as_deref(),
                request.payment_hash.as_deref(),
                request.failure_reason.as_deref(),
                &request.created_at.to_rfc3339(),
                &request.updated_at.to_rfc3339(),
            ],
        )?;
        info!(
            "DB: Inserted channel request: id={}, node_id={}, capacity={}, status={}",
            request.id, request.node_id, request.capacity, request.status
        );
        Ok(())
    }

    /// Get a channel request by ID
    pub async fn get_by_id(&self, id: &str) -> Result<Option<ChannelRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, node_id, host, port, capacity, fee, require_zeroconf, status, channel_id, payment_hash, failure_reason, created_at, updated_at FROM channel_requests WHERE id = ?1"
        )?;
        
        let result = stmt.query_row(rusqlite::params![id], |row| {
            // Handle boolean that might be stored as text or integer
            let require_zeroconf: bool = match row.get_ref(6)? {
                rusqlite::types::ValueRef::Integer(i) => i != 0,
                rusqlite::types::ValueRef::Text(t) => {
                    t.eq_ignore_ascii_case(b"true") || t == b"1"
                }
                _ => false,
            };
            
            Ok(ChannelRequestModel {
                id: row.get(0)?,
                node_id: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                capacity: row.get(4)?,
                fee: row.get(5)?,
                require_zeroconf,
                status: row.get(7)?,
                channel_id: row.get(8)?,
                payment_hash: row.get(9)?,
                failure_reason: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        }).optional()?;

        Ok(result)
    }

    /// Get a channel request by payment hash
    pub async fn get_by_payment_hash(&self, payment_hash: &str) -> Result<Option<ChannelRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, node_id, host, port, capacity, fee, require_zeroconf, status, channel_id, payment_hash, failure_reason, created_at, updated_at FROM channel_requests WHERE payment_hash = ?1"
        )?;
        
        let result = stmt.query_row(rusqlite::params![payment_hash], |row| {
            // Handle boolean that might be stored as text or integer
            let require_zeroconf: bool = match row.get_ref(6)? {
                rusqlite::types::ValueRef::Integer(i) => i != 0,
                rusqlite::types::ValueRef::Text(t) => {
                    t.eq_ignore_ascii_case(b"true") || t == b"1"
                }
                _ => false,
            };
            
            Ok(ChannelRequestModel {
                id: row.get(0)?,
                node_id: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                capacity: row.get(4)?,
                fee: row.get(5)?,
                require_zeroconf,
                status: row.get(7)?,
                channel_id: row.get(8)?,
                payment_hash: row.get(9)?,
                failure_reason: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        }).optional()?;

        Ok(result)
    }

    /// Get channel request by channel_id
    pub async fn get_by_channel_id(&self, channel_id: &str) -> Result<Option<ChannelRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, node_id, host, port, capacity, fee, require_zeroconf, status, channel_id, payment_hash, failure_reason, created_at, updated_at FROM channel_requests WHERE channel_id = ?1"
        )?;
        
        let result = stmt.query_row(rusqlite::params![channel_id], |row| {
            // Handle boolean that might be stored as text or integer
            let require_zeroconf: bool = match row.get_ref(6)? {
                rusqlite::types::ValueRef::Integer(i) => i != 0,
                rusqlite::types::ValueRef::Text(t) => {
                    t.eq_ignore_ascii_case(b"true") || t == b"1"
                }
                _ => false,
            };
            
            Ok(ChannelRequestModel {
                id: row.get(0)?,
                node_id: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                capacity: row.get(4)?,
                fee: row.get(5)?,
                require_zeroconf,
                status: row.get(7)?,
                channel_id: row.get(8)?,
                payment_hash: row.get(9)?,
                failure_reason: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        }).optional()?;

        Ok(result)
    }

    /// Update channel request status
    pub async fn update_status(
        &self,
        id: &str,
        status: &str,
        channel_id: Option<&str>,
    ) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            r#"
            UPDATE channel_requests
            SET status = ?1, channel_id = ?2, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?3
            "#,
            rusqlite::params![status, channel_id, id],
        )?;
        info!(
            "DB: Updated channel request status: id={}, status={}, channel_id={:?}",
            id, status, channel_id
        );
        Ok(())
    }

    /// Update channel request status with failure reason
    pub async fn update_status_with_reason(
        &self,
        id: &str,
        status: &str,
        channel_id: Option<&str>,
        failure_reason: Option<&str>,
    ) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            r#"
            UPDATE channel_requests
            SET status = ?1, channel_id = ?2, failure_reason = ?3, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?4
            "#,
            rusqlite::params![status, channel_id, failure_reason, id],
        )?;
        info!(
            "DB: Updated channel request status with reason: id={}, status={}, reason={:?}",
            id, status, failure_reason
        );
        Ok(())
    }

    /// Update payment_hash for a channel request
    pub async fn update_payment_hash(
        &self,
        id: &str,
        payment_hash: &str,
    ) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            r#"
            UPDATE channel_requests
            SET payment_hash = ?1, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?2
            "#,
            rusqlite::params![payment_hash, id],
        )?;
        Ok(())
    }

    /// List channel requests by status
    pub async fn list_by_status(&self, status: &str) -> Result<Vec<ChannelRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, node_id, host, port, capacity, fee, require_zeroconf, status, channel_id, payment_hash, failure_reason, created_at, updated_at FROM channel_requests WHERE status = ?1 ORDER BY created_at DESC"
        )?;
        
        let results = stmt.query_map(rusqlite::params![status], |row| {
            // Handle boolean that might be stored as text or integer
            let require_zeroconf: bool = match row.get_ref(6)? {
                rusqlite::types::ValueRef::Integer(i) => i != 0,
                rusqlite::types::ValueRef::Text(t) => {
                    t.eq_ignore_ascii_case(b"true") || t == b"1"
                }
                _ => false,
            };
            
            Ok(ChannelRequestModel {
                id: row.get(0)?,
                node_id: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                capacity: row.get(4)?,
                fee: row.get(5)?,
                require_zeroconf,
                status: row.get(7)?,
                channel_id: row.get(8)?,
                payment_hash: row.get(9)?,
                failure_reason: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })?;

        let mut requests = Vec::new();
        for result in results {
            requests.push(result?);
        }

        Ok(requests)
    }

    /// Delete a channel request by ID
    pub async fn delete(&self, id: &str) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            "DELETE FROM channel_requests WHERE id = ?1",
            rusqlite::params![id],
        )?;
        info!("DB: Deleted channel request: id={}", id);
        Ok(())
    }
}

/// Payment queries
pub struct PaymentQueries<'a> {
    db: &'a Database,
}

impl<'a> PaymentQueries<'a> {
    /// Create a new query instance
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Insert a new payment
    pub async fn insert(&self, payment: &PaymentModel) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO payments (id, payment_hash, request_id, amount_msat, status, preimage, created_at, settled_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            [
                &payment.id,
                &payment.payment_hash,
                payment.request_id.as_deref().unwrap_or(""),
                &payment.amount_msat.to_string(),
                &payment.status,
                payment.preimage.as_deref().unwrap_or(""),
                &payment.created_at.to_rfc3339(),
                &payment.settled_at.as_ref().map(|d| d.to_rfc3339()).unwrap_or_default(),
            ],
        )?;
        info!(
            "DB: Inserted payment: id={}, payment_hash={}, amount={} msat, status={}",
            payment.id, payment.payment_hash, payment.amount_msat, payment.status
        );
        Ok(())
    }

    /// Get a payment by hash
    pub async fn get_by_hash(&self, payment_hash: &str) -> Result<Option<PaymentModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT * FROM payments WHERE payment_hash = ?1"
        )?;
        
        let result = stmt.query_row([payment_hash], |row| {
            Ok(PaymentModel {
                id: row.get(0)?,
                payment_hash: row.get(1)?,
                request_id: row.get(2)?,
                amount_msat: row.get(3)?,
                status: row.get(4)?,
                preimage: row.get(5)?,
                created_at: row.get(6)?,
                settled_at: row.get(7)?,
            })
        }).optional()?;

        Ok(result)
    }

    /// Update payment status
    pub async fn update_status(
        &self,
        payment_hash: &str,
        status: &str,
        preimage: Option<&str>,
    ) -> Result<()> {
        let settled_at = if status == "settled" {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        };

        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            r#"
            UPDATE payments
            SET status = ?1, preimage = ?2, settled_at = ?3
            WHERE payment_hash = ?4
            "#,
            [status, preimage.unwrap_or(""), settled_at.as_deref().unwrap_or(""), payment_hash],
        )?;
        info!(
            "DB: Updated payment status: payment_hash={}, status={}",
            payment_hash, status
        );
        Ok(())
    }
}

/// Peer queries
pub struct PeerQueries<'a> {
    db: &'a Database,
}

impl<'a> PeerQueries<'a> {
    /// Create a new query instance
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Get or create a peer
    pub async fn get_or_create(&self, node_id: &str) -> Result<PeerModel> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT * FROM peers WHERE node_id = ?1"
        )?;
        
        let existing = stmt.query_row([node_id], |row| {
            Ok(PeerModel {
                node_id: row.get(0)?,
                alias: row.get(1)?,
                first_seen: row.get(2)?,
                last_connected: row.get(3)?,
                trust_score: row.get(4)?,
                total_channels: row.get(5)?,
                zeroconf_violations: row.get(6)?,
            })
        }).optional()?;

        if let Some(peer) = existing {
            return Ok(peer);
        }

        let peer = PeerModel {
            node_id: node_id.to_string(),
            alias: None,
            first_seen: chrono::Utc::now(),
            last_connected: None,
            trust_score: "new".to_string(),
            total_channels: 0,
            zeroconf_violations: 0,
        };

        conn.execute(
            r#"
            INSERT INTO peers (node_id, alias, first_seen, last_connected, trust_score, total_channels, zeroconf_violations)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            [
                &peer.node_id,
                peer.alias.as_deref().unwrap_or(""),
                &peer.first_seen.to_rfc3339(),
                &peer.last_connected.as_ref().map(|d| d.to_rfc3339()).unwrap_or_default(),
                &peer.trust_score,
                &peer.total_channels.to_string(),
                &peer.zeroconf_violations.to_string(),
            ],
        )?;

        info!("DB: Created new peer: node_id={}", node_id);

        Ok(peer)
    }

    /// Update peer trust score
    pub async fn update_trust_score(&self, node_id: &str, trust_score: &str) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            "UPDATE peers SET trust_score = ?1 WHERE node_id = ?2",
            [trust_score, node_id],
        )?;
        Ok(())
    }

    /// Increment peer channel count
    pub async fn increment_channels(&self, node_id: &str) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            "UPDATE peers SET total_channels = total_channels + 1 WHERE node_id = ?1",
            [node_id],
        )?;
        Ok(())
    }
}

/// Splice request queries
pub struct SpliceRequestQueries<'a> {
    db: &'a Database,
}

impl<'a> SpliceRequestQueries<'a> {
    /// Create a new query instance
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Insert a new splice request
    pub async fn insert(&self, request: &SpliceRequestModel) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO splice_requests (id, channel_id, operation_type, amount, address, status, txid, payment_hash, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            rusqlite::params![
                &request.id,
                &request.channel_id,
                &request.operation_type,
                request.amount,
                request.address.as_deref(),
                &request.status,
                request.txid.as_deref(),
                request.payment_hash.as_deref(),
                &request.created_at.to_rfc3339(),
                &request.updated_at.to_rfc3339(),
            ],
        )?;
        info!(
            "DB: Inserted splice request: id={}, channel_id={}, amount={}, status={}",
            request.id, request.channel_id, request.amount, request.status
        );
        Ok(())
    }

    /// Get a splice request by ID
    pub async fn get_by_id(&self, id: &str) -> Result<Option<SpliceRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, channel_id, operation_type, amount, address, status, txid, payment_hash, created_at, updated_at FROM splice_requests WHERE id = ?1"
        )?;
        
        let result = stmt.query_row(rusqlite::params![id], |row| {
            Ok(SpliceRequestModel {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                operation_type: row.get(2)?,
                amount: row.get(3)?,
                address: row.get(4)?,
                status: row.get(5)?,
                txid: row.get(6)?,
                payment_hash: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        }).optional()?;

        Ok(result)
    }

    /// Get a splice request by payment hash
    pub async fn get_by_payment_hash(&self, payment_hash: &str) -> Result<Option<SpliceRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, channel_id, operation_type, amount, address, status, txid, payment_hash, created_at, updated_at FROM splice_requests WHERE payment_hash = ?1"
        )?;
        
        let result = stmt.query_row(rusqlite::params![payment_hash], |row| {
            Ok(SpliceRequestModel {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                operation_type: row.get(2)?,
                amount: row.get(3)?,
                address: row.get(4)?,
                status: row.get(5)?,
                txid: row.get(6)?,
                payment_hash: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        }).optional()?;

        Ok(result)
    }

    /// Update splice request status
    pub async fn update_status(
        &self,
        id: &str,
        status: &str,
        txid: Option<&str>,
    ) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            r#"
            UPDATE splice_requests
            SET status = ?1, txid = ?2, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?3
            "#,
            rusqlite::params![status, txid, id],
        )?;
        info!(
            "DB: Updated splice request status: id={}, status={}, txid={:?}",
            id, status, txid
        );
        Ok(())
    }

    /// List splice requests by status
    pub async fn list_by_status(&self, status: &str) -> Result<Vec<SpliceRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, channel_id, operation_type, amount, address, status, txid, payment_hash, created_at, updated_at FROM splice_requests WHERE status = ?1 ORDER BY created_at DESC"
        )?;
        
        let results = stmt.query_map(rusqlite::params![status], |row| {
            Ok(SpliceRequestModel {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                operation_type: row.get(2)?,
                amount: row.get(3)?,
                address: row.get(4)?,
                status: row.get(5)?,
                txid: row.get(6)?,
                payment_hash: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;

        let mut requests = Vec::new();
        for result in results {
            requests.push(result?);
        }

        Ok(requests)
    }
}
