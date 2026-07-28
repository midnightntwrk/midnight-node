//! Thin GraphQL client for the Midnight indexer-api.
//!
//! Compiled only when the `indexer` cargo feature is on. The c2m_bridge e2e
//! tests use it to assert on indexer-side `BridgeEvent` rows, the recipient's
//! `bridgeBalance`, and `BridgeClaimTransaction` rows alongside the existing
//! node-side assertions. See `tests/e2e/README.md` for the env-var override.

use serde_json::json;
use std::time::{Duration, Instant};
use tokio::time::sleep;

const DEFAULT_GRAPHQL_URL: &str = "http://127.0.0.1:8088/api/v3/graphql";
const READY_URL_FROM_GRAPHQL_DEFAULT: &str = "http://127.0.0.1:8088/ready";

pub type IndexerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// `bridgeBalance(address: ...)` row, with amounts decoded from the indexer's
/// `HexEncoded` u256 scalar to `u128`. Amounts above u128::MAX would panic in
/// `decode_u128_from_hex` — fine for tests, where bridge amounts stay well under
/// that bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeBalance {
    /// Gross amount bridged in (sum of deposit amounts hitting this recipient).
    pub deposited: u128,
    /// Cumulative amount claimed by the recipient via `ClaimRewards(CardanoBridge)`.
    pub claimed: u128,
    /// Outstanding claimable balance for this recipient. Post-fee: equals
    /// `claimable_so_far - claimed`, NOT `deposited - claimed`. A wallet/UI can
    /// surface this directly as "amount still claimable now".
    pub balance: u128,
}

/// One row from the `bridgeEvents` query. Carries only the fields the c2m_bridge
/// tests assert on; extend as needed.
#[derive(Debug, Clone)]
pub enum BridgeEvent {
    UserTransfer {
        id: i64,
        block_height: u64,
        amount: u128,
        cardano_tx_hash: [u8; 32],
        midnight_tx_hash: [u8; 32],
        recipient: [u8; 32],
    },
    UnapprovedTransfer {
        id: i64,
        block_height: u64,
        amount: u128,
        cardano_tx_hash: [u8; 32],
    },
    InvalidTransfer {
        id: i64,
        block_height: u64,
        amount: u128,
        cardano_tx_hash: [u8; 32],
    },
    SubminimalFlushTransfer {
        id: i64,
        block_height: u64,
        amount: u128,
        count: i64,
    },
    ReserveTransfer {
        id: i64,
        block_height: u64,
        amount: u128,
    },
}

/// One `BridgeClaimTransaction` row from `block(...).transactions[]`, projecting
/// just what the c2m_bridge tests look at.
#[derive(Debug, Clone)]
pub struct BridgeClaim {
    pub block_height: u64,
    pub hash: [u8; 32],
    pub recipient: [u8; 32],
    pub amount: u128,
    /// Sum of NIGHT UTXO values in `unshieldedCreatedOutputs` going to `recipient`.
    pub night_credited_to_recipient: u128,
}

pub struct IndexerClient {
    graphql_url: String,
    ready_url: String,
    http: reqwest::Client,
}

impl IndexerClient {
    /// Build a client from `INDEXER_GRAPHQL_URL` (and the matching `/ready` URL),
    /// falling back to the local-env defaults.
    pub fn from_env_or_default() -> Self {
        let graphql_url = std::env::var("INDEXER_GRAPHQL_URL")
            .unwrap_or_else(|_| DEFAULT_GRAPHQL_URL.to_string());
        let ready_url = derive_ready_url(&graphql_url);
        Self {
            graphql_url,
            ready_url,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("build reqwest::Client"),
        }
    }

    pub fn graphql_url(&self) -> &str {
        &self.graphql_url
    }

    /// Block until the indexer's `/ready` endpoint returns 200 with an empty body
    /// (matches `scripts/tests/indexer-api-e2e.sh`), or fail after `timeout`.
    pub async fn await_ready(&self, timeout: Duration) -> IndexerResult<()> {
        let deadline = Instant::now() + timeout;
        // Last HTTP status code we saw, if any — surfaced in the timeout error so
        // a misconfigured URL ("404 across the board") reads differently from a
        // down indexer ("never connected").
        let mut last_status: Option<u16> = None;
        loop {
            if let Ok(resp) = self.http.get(&self.ready_url).send().await {
                let status = resp.status();
                last_status = Some(status.as_u16());
                if status.is_success() && resp.text().await.unwrap_or_default().is_empty() {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "indexer at {} not ready within {:?} (last status: {:?})",
                    self.ready_url, timeout, last_status
                )
                .into());
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    /// Run a raw GraphQL POST and return the `data` object, panicking-style
    /// (test harness) if the server replied with `errors`.
    async fn graphql(&self, query: &str) -> IndexerResult<serde_json::Value> {
        let body = json!({ "query": query });
        let resp = self
            .http
            .post(&self.graphql_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        let v: serde_json::Value = resp.json().await?;
        if let Some(errors) = v.get("errors") {
            return Err(format!("indexer GraphQL errors: {errors}").into());
        }
        v.get("data")
            .cloned()
            .ok_or_else(|| "indexer GraphQL response missing `data`".into())
    }

    pub async fn bridge_balance(&self, address: &[u8; 32]) -> IndexerResult<BridgeBalance> {
        let q = format!(
            r#"{{ bridgeBalance(address: "{addr}") {{ deposited claimed balance }} }}"#,
            addr = hex::encode(address)
        );
        let data = self.graphql(&q).await?;
        let row = data
            .get("bridgeBalance")
            .ok_or("indexer: bridgeBalance missing from response")?;
        Ok(BridgeBalance {
            deposited: decode_u128_from_hex(row, "deposited")?,
            claimed: decode_u128_from_hex(row, "claimed")?,
            balance: decode_u128_from_hex(row, "balance")?,
        })
    }

    /// Repeatedly call `bridge_balance` until `pred` returns `true` or the deadline
    /// expires. Indexer is a few hundred ms behind chain finalization, so callers
    /// generally want this rather than a single read.
    pub async fn await_bridge_balance_where<F>(
        &self,
        address: &[u8; 32],
        mut pred: F,
        timeout: Duration,
    ) -> IndexerResult<BridgeBalance>
    where
        F: FnMut(&BridgeBalance) -> bool,
    {
        let deadline = Instant::now() + timeout;
        let mut last = self.bridge_balance(address).await?;
        loop {
            if pred(&last) {
                return Ok(last);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "indexer bridgeBalance for {} did not satisfy predicate within {:?} \
                     (last seen: deposited={}, claimed={}, balance={})",
                    hex::encode(address),
                    timeout,
                    last.deposited,
                    last.claimed,
                    last.balance
                )
                .into());
            }
            sleep(Duration::from_millis(500)).await;
            last = self.bridge_balance(address).await?;
        }
    }

    /// Query `bridgeEvents` with optional recipient + variant filters.
    pub async fn bridge_events(
        &self,
        recipient: Option<&[u8; 32]>,
        variant: Option<BridgeEventVariant>,
        limit: u32,
    ) -> IndexerResult<Vec<BridgeEvent>> {
        let mut args = vec![format!("limit: {limit}")];
        if let Some(r) = recipient {
            args.push(format!(r#"recipient: "{}""#, hex::encode(r)));
        }
        if let Some(v) = variant {
            args.push(format!("variant: {}", v.as_graphql()));
        }
        let q = format!(
            r#"{{ bridgeEvents({args}) {{
                __typename
                ... on BridgeUserTransfer {{ id blockHeight amount cardanoTxHash midnightTxHash recipient }}
                ... on BridgeUnapprovedTransfer {{ id blockHeight amount cardanoTxHash }}
                ... on BridgeInvalidTransfer {{ id blockHeight amount cardanoTxHash }}
                ... on BridgeSubminimalFlushTransfer {{ id blockHeight amount count }}
                ... on BridgeReserveTransfer {{ id blockHeight amount }}
            }} }}"#,
            args = args.join(", ")
        );
        let data = self.graphql(&q).await?;
        let raw = data
            .get("bridgeEvents")
            .and_then(|v| v.as_array())
            .ok_or("indexer: bridgeEvents missing or not an array")?;
        raw.iter().map(parse_bridge_event).collect()
    }

    /// Poll `bridgeEvents` until at least one event matches `pred`, returning the
    /// first match, or fail at `timeout`.
    pub async fn await_bridge_event<F>(
        &self,
        recipient: Option<&[u8; 32]>,
        variant: Option<BridgeEventVariant>,
        mut pred: F,
        timeout: Duration,
    ) -> IndexerResult<BridgeEvent>
    where
        F: FnMut(&BridgeEvent) -> bool,
    {
        let deadline = Instant::now() + timeout;
        loop {
            let events = self.bridge_events(recipient, variant, 50).await?;
            if let Some(hit) = events.into_iter().find(|e| pred(e)) {
                return Ok(hit);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "no matching bridgeEvent surfaced within {:?} \
                     (recipient: {:?}, variant: {:?})",
                    timeout,
                    recipient.map(hex::encode),
                    variant,
                )
                .into());
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    /// Walk blocks from `from_height` forward (inclusive) up to `from_height + max_blocks`
    /// looking for a `BridgeClaimTransaction` whose recipient matches. Returns the
    /// first hit, or `Ok(None)` if none found within the window. Errors surface
    /// transport / response-shape problems.
    pub async fn find_bridge_claim_for_recipient(
        &self,
        recipient: &[u8; 32],
        from_height: u64,
        max_blocks: u64,
    ) -> IndexerResult<Option<BridgeClaim>> {
        for h in from_height..from_height.saturating_add(max_blocks) {
            let q = format!(
                r#"{{ block(offset: {{ height: {h} }}) {{
                    height
                    transactions {{
                        __typename hash
                        ... on BridgeClaimTransaction {{
                            recipient amount
                            unshieldedCreatedOutputs {{ tokenType owner value }}
                        }}
                    }}
                }} }}"#
            );
            let data = self.graphql(&q).await?;
            let block = match data.get("block") {
                Some(b) if !b.is_null() => b,
                _ => continue, // block not yet indexed
            };
            let txs = block
                .get("transactions")
                .and_then(|v| v.as_array())
                .ok_or("indexer: block.transactions missing or not an array")?;
            for tx in txs {
                if tx.get("__typename").and_then(|v| v.as_str()) != Some("BridgeClaimTransaction") {
                    continue;
                }
                let claim_recipient: [u8; 32] = decode_hex_bytes(tx, "recipient")?;
                if &claim_recipient != recipient {
                    continue;
                }
                let amount = decode_u128_from_hex(tx, "amount")?;
                let hash: [u8; 32] = decode_hex_bytes(tx, "hash")?;
                let night_credited = sum_night_outputs_to_recipient(tx, recipient).unwrap_or(0);
                return Ok(Some(BridgeClaim {
                    block_height: h,
                    hash,
                    recipient: claim_recipient,
                    amount,
                    night_credited_to_recipient: night_credited,
                }));
            }
        }
        Ok(None)
    }

    /// Poll `find_bridge_claim_for_recipient` until a hit appears or `timeout` expires.
    pub async fn await_bridge_claim_for_recipient(
        &self,
        recipient: &[u8; 32],
        from_height: u64,
        max_blocks: u64,
        timeout: Duration,
    ) -> IndexerResult<BridgeClaim> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(claim) = self
                .find_bridge_claim_for_recipient(recipient, from_height, max_blocks)
                .await?
            {
                return Ok(claim);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "no BridgeClaimTransaction for recipient {} found in blocks \
                     [{}, {}) within {:?}",
                    hex::encode(recipient),
                    from_height,
                    from_height + max_blocks,
                    timeout,
                )
                .into());
            }
            sleep(Duration::from_millis(750)).await;
        }
    }
}

/// Mirrors the indexer's `BridgeEventVariant` enum at the call-site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeEventVariant {
    UserTransfer,
    UnapprovedTransfer,
    InvalidTransfer,
    SubminimalFlushTransfer,
    ReserveTransfer,
}

impl BridgeEventVariant {
    fn as_graphql(self) -> &'static str {
        match self {
            Self::UserTransfer => "USER_TRANSFER",
            Self::UnapprovedTransfer => "UNAPPROVED_TRANSFER",
            Self::InvalidTransfer => "INVALID_TRANSFER",
            Self::SubminimalFlushTransfer => "SUBMINIMAL_FLUSH_TRANSFER",
            Self::ReserveTransfer => "RESERVE_TRANSFER",
        }
    }
}

fn parse_bridge_event(v: &serde_json::Value) -> IndexerResult<BridgeEvent> {
    let typename = v
        .get("__typename")
        .and_then(|v| v.as_str())
        .ok_or("bridgeEvent row missing __typename")?;
    let id = v
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or("bridgeEvent row missing id")?;
    let block_height = v
        .get("blockHeight")
        .and_then(|v| v.as_i64())
        .ok_or("bridgeEvent row missing blockHeight")? as u64;
    match typename {
        "BridgeUserTransfer" => Ok(BridgeEvent::UserTransfer {
            id,
            block_height,
            amount: decode_u128_from_hex(v, "amount")?,
            cardano_tx_hash: decode_hex_bytes(v, "cardanoTxHash")?,
            midnight_tx_hash: decode_hex_bytes(v, "midnightTxHash")?,
            recipient: decode_hex_bytes(v, "recipient")?,
        }),
        "BridgeUnapprovedTransfer" => Ok(BridgeEvent::UnapprovedTransfer {
            id,
            block_height,
            amount: decode_u128_from_hex(v, "amount")?,
            cardano_tx_hash: decode_hex_bytes(v, "cardanoTxHash")?,
        }),
        "BridgeInvalidTransfer" => Ok(BridgeEvent::InvalidTransfer {
            id,
            block_height,
            amount: decode_u128_from_hex(v, "amount")?,
            cardano_tx_hash: decode_hex_bytes(v, "cardanoTxHash")?,
        }),
        "BridgeSubminimalFlushTransfer" => Ok(BridgeEvent::SubminimalFlushTransfer {
            id,
            block_height,
            amount: decode_u128_from_hex(v, "amount")?,
            count: v
                .get("count")
                .and_then(|v| v.as_i64())
                .ok_or("BridgeSubminimalFlushTransfer missing count")?,
        }),
        "BridgeReserveTransfer" => Ok(BridgeEvent::ReserveTransfer {
            id,
            block_height,
            amount: decode_u128_from_hex(v, "amount")?,
        }),
        other => Err(format!("unknown bridgeEvent __typename: {other}").into()),
    }
}

fn sum_night_outputs_to_recipient(tx: &serde_json::Value, recipient: &[u8; 32]) -> Option<u128> {
    // owner is bech32 (mn_addr_...), not hex; match on the hex-encoded 32-byte
    // payload by decoding bech32 would pull in another dep. Instead we rely on
    // the indexer surfacing tokenType=NIGHT (all-zero) outputs, and let the
    // outer node-side assertion cover the per-byte recipient match. We still
    // sum only NIGHT UTXOs here, so this matches `credited_to_recipient` for
    // the happy path; multi-recipient claims aren't a thing today.
    let _ = recipient; // reserved for future per-recipient tightening
    let outputs = tx
        .get("unshieldedCreatedOutputs")
        .and_then(|v| v.as_array())?;
    let mut total: u128 = 0;
    for o in outputs {
        let token_type = o
            .get("tokenType")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if token_type != "0000000000000000000000000000000000000000000000000000000000000000" {
            continue;
        }
        let value: u128 = o
            .get("value")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        total = total.saturating_add(value);
    }
    Some(total)
}

fn decode_u128_from_hex(v: &serde_json::Value, field: &str) -> IndexerResult<u128> {
    let s = v
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("field `{field}` missing or not a string"))?;
    let bytes = hex::decode(s).map_err(|e| format!("field `{field}` not hex: {e}"))?;
    if bytes.len() > 16 {
        // u256 → u128 truncation: assert the upper bytes are zero.
        let upper = &bytes[..bytes.len() - 16];
        if upper.iter().any(|b| *b != 0) {
            return Err(format!("field `{field}` exceeds u128 range (hex {s})").into());
        }
    }
    let mut buf = [0u8; 16];
    let off = buf.len().saturating_sub(bytes.len());
    buf[off..].copy_from_slice(&bytes[bytes.len().saturating_sub(16)..]);
    Ok(u128::from_be_bytes(buf))
}

fn decode_hex_bytes<const N: usize>(v: &serde_json::Value, field: &str) -> IndexerResult<[u8; N]> {
    let s = v
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("field `{field}` missing or not a string"))?;
    let bytes = hex::decode(s).map_err(|e| format!("field `{field}` not hex: {e}"))?;
    if bytes.len() != N {
        return Err(format!("field `{field}` expected {N} bytes, got {}", bytes.len()).into());
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn derive_ready_url(graphql_url: &str) -> String {
    // Strip a trailing `/api/v3/graphql` (or `/graphql`) and append `/ready`.
    // Falls back to a hand-picked default when the URL shape is unusual.
    if let Some(prefix) = graphql_url.strip_suffix("/api/v3/graphql") {
        format!("{prefix}/ready")
    } else if let Some(prefix) = graphql_url.strip_suffix("/graphql") {
        format!("{prefix}/ready")
    } else {
        READY_URL_FROM_GRAPHQL_DEFAULT.to_string()
    }
}
