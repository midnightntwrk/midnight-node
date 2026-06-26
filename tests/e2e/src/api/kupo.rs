//! Thin HTTP client for [kupo](https://github.com/CardanoSolutions/kupo), the
//! Cardano chain-indexer bundled in local-env.
//!
//! The cross-chain bridge invariant suite uses it to read the **total** cNIGHT in
//! existence (summed across all unspent UTxOs by minting policy) — the
//! `minted_total` from which the unlocked Cardano pool is derived:
//! `C.U = minted_total - C.L - C.R`. Per-address pools (`C.L` at the ICS validator,
//! `C.R` at the Reserve validator) are read via ogmios in `CardanoClient`.

use std::time::Duration;

pub type KupoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Minimal kupo client over its REST `/matches` API.
pub struct KupoClient {
    base_url: String,
    http: reqwest::Client,
}

impl KupoClient {
    /// Build a client for `base_url` (e.g. `http://127.0.0.1:1442`).
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("build reqwest::Client"),
        }
    }

    /// Total unspent quantity of all assets under `policy_id` across the whole chain.
    /// With no burning this equals the total minted supply (`minted_total`).
    ///
    /// Queries `GET /matches/{policy_id}.*?unspent` and sums every `value.assets`
    /// entry whose key starts with `policy_id` (kupo keys assets as
    /// `"<policy_id>.<asset_name_hex>"`).
    pub async fn cnight_total(&self, policy_id: &str) -> KupoResult<u128> {
        let url = format!("{}/matches/{}.*?unspent", self.base_url, policy_id);
        self.sum_policy(&url, policy_id).await
    }

    async fn sum_policy(&self, url: &str, policy_id: &str) -> KupoResult<u128> {
        let matches: Vec<serde_json::Value> = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut total: u128 = 0;
        for m in &matches {
            let Some(assets) = m
                .get("value")
                .and_then(|v| v.get("assets"))
                .and_then(|a| a.as_object())
            else {
                continue;
            };
            for (unit, amount) in assets {
                if unit.starts_with(policy_id) {
                    // kupo emits native-token quantities as JSON integers; serde_json
                    // keeps full u64 precision (no f64 rounding), and cNIGHT totals
                    // (<= 24e15) sit well within u64.
                    let qty = amount
                        .as_u64()
                        .ok_or_else(|| format!("kupo asset {unit} amount not a u64: {amount}"))?;
                    total = total.saturating_add(qty as u128);
                }
            }
        }
        Ok(total)
    }
}
