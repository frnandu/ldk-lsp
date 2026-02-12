//! Fee estimation module for dynamic onchain fees
//!
//! This module provides dynamic fee estimation by querying external APIs
//! (mempool.space by default) to get current network fee rates.

use tracing::{debug, error, info};

/// Fee estimator for dynamic onchain fees
#[derive(Debug, Clone)]
pub struct FeeEstimator {
    /// Base URL for the fee API
    api_url: String,
    /// Confirmation target in blocks (e.g., 6 for ~1 hour)
    confirmation_target: u32,
}

impl FeeEstimator {
    /// Create a new fee estimator with default mempool.space API
    pub fn new() -> Self {
        Self {
            api_url: "https://mempool.space/api/v1/fees/recommended".to_string(),
            confirmation_target: 6, // ~1 hour
        }
    }

    /// Create a fee estimator with custom API URL
    pub fn with_url(api_url: String) -> Self {
        Self {
            api_url,
            confirmation_target: 6,
        }
    }

    /// Set confirmation target
    pub fn with_confirmation_target(mut self, target: u32) -> Self {
        self.confirmation_target = target;
        self
    }

    /// Get current fee rate in satoshis per vbyte
    pub async fn get_fee_rate(&self) -> anyhow::Result<u64> {
        info!("Fetching dynamic fee rate from {}...", self.api_url);

        let client = reqwest::Client::new();
        let response = client
            .get(&self.api_url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| {
                error!("Failed to fetch fee rate: {}", e);
                anyhow::anyhow!("Failed to fetch fee rate: {}", e)
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("Fee API returned error: {} - {}", status, text);
            return Err(anyhow::anyhow!(
                "Fee API returned error: {} - {}",
                status,
                text
            ));
        }

        let fee_response: FeeResponse = response.json().await.map_err(|e| {
            error!("Failed to parse fee response: {}", e);
            anyhow::anyhow!("Failed to parse fee response: {}", e)
        })?;

        let fee_rate = self.select_fee_rate(&fee_response);
        info!("Dynamic fee rate: {} sat/vbyte", fee_rate);

        Ok(fee_rate)
    }

    /// Estimate fee for a transaction with given vbytes
    pub async fn estimate_fee(&self, vbytes: u64) -> anyhow::Result<u64> {
        let fee_rate = self.get_fee_rate().await?;
        let fee = vbytes.saturating_mul(fee_rate);
        debug!("Estimated fee: {} sats ({} vbytes @ {} sat/vbyte)", fee, vbytes, fee_rate);
        Ok(fee)
    }

    /// Select appropriate fee rate based on confirmation target
    fn select_fee_rate(&self, response: &FeeResponse) -> u64 {
        let rate = match self.confirmation_target {
            0..=1 => response.fastest_fee,        // ~10 min
            2..=3 => response.half_hour_fee,     // ~30 min
            4..=6 => response.hour_fee,          // ~1 hour
            7..=12 => response.economy_fee,      // ~2 hours
            _ => response.minimum_fee,           // ~24 hours
        };

        // Add 20% buffer to account for fee fluctuations between quote and execution
        let rate_with_buffer = rate.saturating_mul(120) / 100;
        
        // Ensure minimum fee of 1 sat/vbyte
        rate_with_buffer.max(1)
    }
}

impl Default for FeeEstimator {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from mempool.space API
#[derive(Debug, Clone, serde::Deserialize)]
struct FeeResponse {
    /// Fastest fee (~10 min)
    #[serde(rename = "fastestFee")]
    fastest_fee: u64,
    /// Half hour fee (~30 min)
    #[serde(rename = "halfHourFee")]
    half_hour_fee: u64,
    /// Hour fee (~1 hour)
    #[serde(rename = "hourFee")]
    hour_fee: u64,
    /// Economy fee (~2 hours)
    #[serde(rename = "economyFee")]
    economy_fee: u64,
    /// Minimum fee (~24 hours)
    #[serde(rename = "minimumFee")]
    minimum_fee: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_estimator_default() {
        let estimator = FeeEstimator::default();
        assert_eq!(estimator.confirmation_target, 6);
    }

    #[test]
    fn test_select_fee_rate() {
        let estimator = FeeEstimator::new();
        let response = FeeResponse {
            fastest_fee: 50,
            half_hour_fee: 30,
            hour_fee: 20,
            economy_fee: 10,
            minimum_fee: 5,
        };

        // ~1 hour should use hour_fee (20) + 20% buffer = 24
        let rate = estimator.select_fee_rate(&response);
        assert_eq!(rate, 24);
    }
}
