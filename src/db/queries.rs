//! Database queries

use super::{Database, PeerModel};
use anyhow::Result;
use rusqlite::OptionalExtension;
use tracing::info;

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

/// Receive request queries for JIT liquidity
pub struct ReceiveRequestQueries<'a> {
    db: &'a Database,
}

impl<'a> ReceiveRequestQueries<'a> {
    /// Create a new query instance
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Insert a new receive request
    pub async fn insert(&self, request: &crate::db::ReceiveRequestModel) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO receive_requests (id, node_id, host, port, amount, fee_base, fee_ppm, fee_onchain, fee_total, reserve_amount, fee_rate, total_invoice_amount, total_channel_capacity, is_splice, channel_id, status, payment_hash, user_invoice, user_payment_hash, user_invoice_paid, failure_reason, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
            "#,
            rusqlite::params![
                &request.id,
                &request.node_id,
                &request.host,
                request.port,
                request.amount,
                request.fee_base,
                request.fee_ppm,
                request.fee_onchain,
                request.fee_total,
                request.reserve_amount,
                request.fee_rate,
                request.total_invoice_amount,
                request.total_channel_capacity,
                request.is_splice,
                request.channel_id.as_deref(),
                &request.status,
                request.payment_hash.as_deref(),
                request.user_invoice.as_deref(),
                request.user_payment_hash.as_deref(),
                request.user_invoice_paid,
                request.failure_reason.as_deref(),
                &request.created_at.to_rfc3339(),
                &request.updated_at.to_rfc3339(),
            ],
        )?;
        info!(
            "DB: Inserted receive request: id={}, node_id={}, amount={}, is_splice={}, status={}",
            request.id, request.node_id, request.amount, request.is_splice, request.status
        );
        Ok(())
    }

    /// Get a receive request by ID
    pub async fn get_by_id(&self, id: &str) -> Result<Option<crate::db::ReceiveRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, node_id, host, port, amount, fee_base, fee_ppm, fee_onchain, fee_total, reserve_amount, fee_rate, total_invoice_amount, total_channel_capacity, is_splice, channel_id, status, payment_hash, user_invoice, user_payment_hash, user_invoice_paid, failure_reason, created_at, updated_at FROM receive_requests WHERE id = ?1"
        )?;
        
        let result = stmt.query_row(rusqlite::params![id], |row| {
            Ok(crate::db::ReceiveRequestModel {
                id: row.get(0)?,
                node_id: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                amount: row.get(4)?,
                fee_base: row.get(5)?,
                fee_ppm: row.get(6)?,
                fee_onchain: row.get(7)?,
                fee_total: row.get(8)?,
                reserve_amount: row.get(9)?,
                fee_rate: row.get(10)?,
                total_invoice_amount: row.get(11)?,
                total_channel_capacity: row.get(12)?,
                is_splice: row.get::<_, i32>(13)? != 0,
                channel_id: row.get(14)?,
                status: row.get(15)?,
                payment_hash: row.get(16)?,
                user_invoice: row.get(17)?,
                user_payment_hash: row.get(18)?,
                user_invoice_paid: row.get::<_, i32>(19)? != 0,
                failure_reason: row.get(20)?,
                created_at: row.get(21)?,
                updated_at: row.get(22)?,
            })
        }).optional()?;

        Ok(result)
    }

    /// Get a receive request by payment hash
    pub async fn get_by_payment_hash(&self, payment_hash: &str) -> Result<Option<crate::db::ReceiveRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, node_id, host, port, amount, fee_base, fee_ppm, fee_onchain, fee_total, reserve_amount, fee_rate, total_invoice_amount, total_channel_capacity, is_splice, channel_id, status, payment_hash, user_invoice, user_payment_hash, user_invoice_paid, failure_reason, created_at, updated_at FROM receive_requests WHERE payment_hash = ?1"
        )?;

        let result = stmt.query_row(rusqlite::params![payment_hash], |row| {
            Ok(crate::db::ReceiveRequestModel {
                id: row.get(0)?,
                node_id: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                amount: row.get(4)?,
                fee_base: row.get(5)?,
                fee_ppm: row.get(6)?,
                fee_onchain: row.get(7)?,
                fee_total: row.get(8)?,
                reserve_amount: row.get(9)?,
                fee_rate: row.get(10)?,
                total_invoice_amount: row.get(11)?,
                total_channel_capacity: row.get(12)?,
                is_splice: row.get::<_, i32>(13)? != 0,
                channel_id: row.get(14)?,
                status: row.get(15)?,
                payment_hash: row.get(16)?,
                user_invoice: row.get(17)?,
                user_payment_hash: row.get(18)?,
                user_invoice_paid: row.get::<_, i32>(19)? != 0,
                failure_reason: row.get(20)?,
                created_at: row.get(21)?,
                updated_at: row.get(22)?,
            })
        }).optional()?;

        Ok(result)
    }

    /// Update receive request status
    /// When channel_id is None, the existing channel_id is preserved (not overwritten to NULL)
    pub async fn update_status(
        &self,
        id: &str,
        status: &str,
        channel_id: Option<&str>,
    ) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        if let Some(cid) = channel_id {
            conn.execute(
                r#"
                UPDATE receive_requests
                SET status = ?1, channel_id = ?2, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?3
                "#,
                rusqlite::params![status, cid, id],
            )?;
        } else {
            conn.execute(
                r#"
                UPDATE receive_requests
                SET status = ?1, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?2
                "#,
                rusqlite::params![status, id],
            )?;
        }
        info!(
            "DB: Updated receive request status: id={}, status={}, channel_id={:?}",
            id, status, channel_id
        );
        Ok(())
    }

    /// Update receive request status with failure reason
    /// When channel_id is None, the existing channel_id is preserved (not overwritten to NULL)
    pub async fn update_status_with_reason(
        &self,
        id: &str,
        status: &str,
        channel_id: Option<&str>,
        failure_reason: Option<&str>,
    ) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        if let Some(cid) = channel_id {
            conn.execute(
                r#"
                UPDATE receive_requests
                SET status = ?1, channel_id = ?2, failure_reason = ?3, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?4
                "#,
                rusqlite::params![status, cid, failure_reason, id],
            )?;
        } else {
            conn.execute(
                r#"
                UPDATE receive_requests
                SET status = ?1, failure_reason = ?2, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?3
                "#,
                rusqlite::params![status, failure_reason, id],
            )?;
        }
        info!(
            "DB: Updated receive request status with reason: id={}, status={}, reason={:?}",
            id, status, failure_reason
        );
        Ok(())
    }

    /// List receive requests by status
    pub async fn list_by_status(&self, status: &str) -> Result<Vec<crate::db::ReceiveRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, node_id, host, port, amount, fee_base, fee_ppm, fee_onchain, fee_total, reserve_amount, fee_rate, total_invoice_amount, total_channel_capacity, is_splice, channel_id, status, payment_hash, user_invoice, user_payment_hash, user_invoice_paid, failure_reason, created_at, updated_at FROM receive_requests WHERE status = ?1 ORDER BY created_at DESC"
        )?;

        let results = stmt.query_map(rusqlite::params![status], |row| {
            Ok(crate::db::ReceiveRequestModel {
                id: row.get(0)?,
                node_id: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                amount: row.get(4)?,
                fee_base: row.get(5)?,
                fee_ppm: row.get(6)?,
                fee_onchain: row.get(7)?,
                fee_total: row.get(8)?,
                reserve_amount: row.get(9)?,
                fee_rate: row.get(10)?,
                total_invoice_amount: row.get(11)?,
                total_channel_capacity: row.get(12)?,
                is_splice: row.get::<_, i32>(13)? != 0,
                channel_id: row.get(14)?,
                status: row.get(15)?,
                payment_hash: row.get(16)?,
                user_invoice: row.get(17)?,
                user_payment_hash: row.get(18)?,
                user_invoice_paid: row.get::<_, i32>(19)? != 0,
                failure_reason: row.get(20)?,
                created_at: row.get(21)?,
                updated_at: row.get(22)?,
            })
        })?;

        let mut requests = Vec::new();
        for result in results {
            requests.push(result?);
        }

        Ok(requests)
    }

    /// Delete a receive request by ID
    pub async fn delete(&self, id: &str) -> Result<()> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        conn.execute(
            "DELETE FROM receive_requests WHERE id = ?1",
            rusqlite::params![id],
        )?;
        info!("DB: Deleted receive request: id={}", id);
        Ok(())
    }

    /// Get receive requests by channel ID and status
    pub async fn get_by_channel_id_and_status(
        &self,
        channel_id: &str,
        status: &str,
    ) -> Result<Vec<crate::db::ReceiveRequestModel>> {
        let conn = self.db.conn().clone();
        let conn = conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, node_id, host, port, amount, fee_base, fee_ppm, fee_onchain, fee_total, reserve_amount, fee_rate, total_invoice_amount, total_channel_capacity, is_splice, channel_id, status, payment_hash, user_invoice, user_payment_hash, user_invoice_paid, failure_reason, created_at, updated_at FROM receive_requests WHERE channel_id = ?1 AND status = ?2 ORDER BY created_at DESC"
        )?;

        let results = stmt.query_map(rusqlite::params![channel_id, status], |row| {
            Ok(crate::db::ReceiveRequestModel {
                id: row.get(0)?,
                node_id: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                amount: row.get(4)?,
                fee_base: row.get(5)?,
                fee_ppm: row.get(6)?,
                fee_onchain: row.get(7)?,
                fee_total: row.get(8)?,
                reserve_amount: row.get(9)?,
                fee_rate: row.get(10)?,
                total_invoice_amount: row.get(11)?,
                total_channel_capacity: row.get(12)?,
                is_splice: row.get::<_, i32>(13)? != 0,
                channel_id: row.get(14)?,
                status: row.get(15)?,
                payment_hash: row.get(16)?,
                user_invoice: row.get(17)?,
                user_payment_hash: row.get(18)?,
                user_invoice_paid: row.get::<_, i32>(19)? != 0,
                failure_reason: row.get(20)?,
                created_at: row.get(21)?,
                updated_at: row.get(22)?,
            })
        })?;

        let mut requests = Vec::new();
        for result in results {
            requests.push(result?);
        }

        Ok(requests)
    }
}
