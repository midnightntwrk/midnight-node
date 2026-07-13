//! Faucet manager for e2e tests.
//!
//! Backs a pool of `N` long-lived "worker" UTXOs at the shared funded faucet address.
//! Each `request_tokens(addr, lovelace)` call:
//!
//! 1. Acquires one worker via try-lock round-robin (concurrent callers fan out across
//!    workers; if all are busy, the caller awaits on its round-robin slot).
//! 2. Builds a single-input send: `worker -> dest (lovelace) + change (back to faucet)`.
//! 3. Polls for both outputs to confirm on-chain, then atomically replaces the worker
//!    slot with the freshly-confirmed change UTXO before releasing the mutex.
//!
//! Because each worker is independent, parallel callers using different workers never
//! contend on the same on-chain UTXO — that's how we avoid the double-spend / ghost-UTXO
//! races a naive "pick from faucet's UTXO set on every call" scheme is prone to.
//!
//! ## Sizing
//!
//! - **N (worker count)**: defaults to 4. Override with `E2E_FAUCET_WORKERS=<n>` to
//!   right-size for the actual test parallelism — set it equal to `--test-threads` so
//!   each parallel test can claim its own worker. The cnight-observation nightly
//!   workflow runs `--test-threads 16` and sets `E2E_FAUCET_WORKERS=16` accordingly.
//!   Larger N reduces queueing but each worker needs to be large enough to handle
//!   several requests.
//! - **Per-request cap**: 1000 ADA ([`REQUEST_CAP_LOVELACE`]). Larger requests panic loudly.
//! - **Worker funding floor**: each worker UTXO must hold at least 1000 ADA at init.
//!
//! ## Init (`FaucetManager::new`)
//!
//! Queries the faucet address. If at least N UTXOs are already ≥ 1000 ADA, takes the
//! largest N and skips priming. Otherwise builds one "prime" tx that splits the faucet's
//! balance into N equal-size workers. Panics with a clear message if the faucet doesn't
//! hold enough ADA to support N workers.
//!
//! Init runs once per test binary via the `OnceCell` in `tests/lib.rs`.

use crate::api::cardano::CardanoClient;
use ogmios_client::OgmiosClientError;
use ogmios_client::types::OgmiosUtxo;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex as StdMutex, OnceLock};
use tokio::sync::{Mutex, MutexGuard};
use whisky::Asset;

const WORKERS_DEFAULT: usize = 4;
const WORKERS_ENV_KEY: &str = "E2E_FAUCET_WORKERS";

/// Per-request lovelace cap. A `request_tokens` above this panics — protects against tests
/// inadvertently draining a worker in one shot.
const REQUEST_CAP_LOVELACE: u64 = 1_000_000_000; // 1000 ADA

/// Minimum lovelace a worker UTXO must hold at init / after refresh.
/// local-env: equal to the request cap (the genesis-funded faucet is
/// effectively unlimited). Shared networks: much lower — the shared faucet
/// wallet is a finite, slowly-draining resource, and the observation tests'
/// requests are tiny. An oversized request still fails loudly via the
/// drained-worker check.
#[cfg(any(feature = "local", feature = "local-dev", feature = "local-ci"))]
const MIN_WORKER_LOVELACE: u64 = REQUEST_CAP_LOVELACE;
#[cfg(any(feature = "qanet", feature = "devnet"))]
const MIN_WORKER_LOVELACE: u64 = 250_000_000;

/// Lovelace headroom reserved on top of N×MIN_WORKER_LOVELACE when sizing a prime tx —
/// covers the prime tx fee and keeps the change output above min-UTXO.
const PRIME_FEE_BUFFER: u64 = 5_000_000;

/// Lovelace headroom reserved on top of the requested amount when checking worker balance,
/// covers the per-request fee (~0.25 ADA) and keeps the change UTXO above min-UTXO.
const REQUEST_FEE_BUFFER: u64 = 2_000_000;

/// Safety cap: if the faucet has more than this many UTXOs, the prime path pre-consolidates
/// first rather than feeding them all into one split tx (which would exceed Cardano's tx
/// size limit).
const MAX_INPUTS_PER_TX: usize = 50;

/// Number of times the prime tx may be retried while dropping ogmios-reported ghost UTXOs.
const PRIME_MAX_ATTEMPTS: usize = 4;

/// Resolves the configured worker count, reading `E2E_FAUCET_WORKERS` once and caching.
/// Non-numeric or `<1` values fall back to the default.
fn worker_count() -> usize {
    static N: OnceLock<usize> = OnceLock::new();
    *N.get_or_init(|| {
        let n = std::env::var(WORKERS_ENV_KEY)
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n >= 1)
            .unwrap_or(WORKERS_DEFAULT);
        tracing::info!(
            "FaucetManager: using {} worker(s) (override with {})",
            n,
            WORKERS_ENV_KEY
        );
        n
    })
}

/// Concurrent-safe faucet for parallel e2e tests. See the module-level docs for the design.
pub struct FaucetManager {
    faucet: CardanoClient,
    workers: Vec<Mutex<OgmiosUtxo>>,
    next_worker: AtomicUsize,
    /// Refs of every UTXO currently assigned to a worker slot, so a stale
    /// slot's replacement never duplicates a live slot (worker mutexes
    /// can't be peeked). Every slot write goes through
    /// [`Self::replace_worker_slot`].
    assigned: StdMutex<HashSet<([u8; 32], u16)>>,
}

impl FaucetManager {
    /// Build a `FaucetManager` against `faucet`. Eagerly queries the faucet address, then
    /// either adopts N existing UTXOs ≥ 1000 ADA as workers or submits one prime tx to
    /// split the faucet balance into N evenly-sized worker UTXOs.
    ///
    /// Panics (with a clear top-up message) if the faucet doesn't hold enough ADA for N
    /// workers, or if ogmios reports inconsistent UTXOs across `PRIME_MAX_ATTEMPTS` retries.
    pub async fn new(faucet: CardanoClient) -> Self {
        let workers = Self::initialise_workers(&faucet).await;
        let assigned = workers
            .iter()
            .map(|u| (u.transaction.id, u.index))
            .collect();
        FaucetManager {
            faucet,
            workers: workers.into_iter().map(Mutex::new).collect(),
            next_worker: AtomicUsize::new(0),
            assigned: StdMutex::new(assigned),
        }
    }

    /// Swap a worker slot's UTXO, keeping the assigned-refs registry in sync.
    fn replace_worker_slot(&self, slot: &mut OgmiosUtxo, new: OgmiosUtxo) {
        let mut assigned = self.assigned.lock().expect("assigned registry poisoned");
        assigned.remove(&(slot.transaction.id, slot.index));
        assigned.insert((new.transaction.id, new.index));
        *slot = new;
    }

    /// Send `lovelace` from the faucet to `address` and return the resulting UTXO at the
    /// destination. The call holds one worker for the duration of the tx confirmation —
    /// other callers grab different workers in parallel — and atomically refreshes the
    /// worker slot with the change UTXO before releasing.
    ///
    /// Panics if `lovelace` exceeds [`REQUEST_CAP_LOVELACE`] (1000 ADA), if the chosen
    /// worker is too drained to satisfy the request, or if the tx never confirms within the
    /// underlying poll budget. Drained workers do not auto-refill; top up the faucet and
    /// re-prime on the next test run.
    pub async fn request_tokens(&self, address: &str, lovelace: u64) -> OgmiosUtxo {
        assert!(
            lovelace <= REQUEST_CAP_LOVELACE,
            "FaucetManager: requested {} lovelace exceeds cap of {} lovelace (1000 ADA)",
            lovelace,
            REQUEST_CAP_LOVELACE,
        );

        let (worker_idx, mut worker) = self.acquire_worker().await;

        let need = lovelace.saturating_add(REQUEST_FEE_BUFFER);
        if worker.value.lovelace < need {
            panic!(
                "FaucetManager: worker[{}] drained to {} lovelace, cannot satisfy {} + {} fee buffer. \
                 Faucet needs top-up or re-prime at {}.",
                worker_idx,
                worker.value.lovelace,
                lovelace,
                REQUEST_FEE_BUFFER,
                self.faucet.address_as_bech32(),
            );
        }

        let assets = vec![Asset::new_from_str("lovelace", &lovelace.to_string())];

        // A concurrent run sharing the faucet wallet can spend this
        // worker's UTXO out from under us; recover by re-acquiring a fresh
        // one from live faucet state. Collisions on the fresh pick retry
        // the same way, so the loop converges or gives up loudly.
        const SEND_MAX_ATTEMPTS: usize = 3;
        let mut attempt = 0;
        let response = loop {
            attempt += 1;
            match self
                .faucet
                .send(std::slice::from_ref(&*worker), address, assets.clone())
                .await
            {
                Ok(r) => break r,
                Err(e)
                    if attempt < SEND_MAX_ATTEMPTS
                        && crate::api::cardano::is_inputs_spent_error(&e) =>
                {
                    tracing::warn!(
                        "FaucetManager: worker[{}] UTXO {}#{} was already spent (concurrent \
                         run sharing the faucet wallet?); re-acquiring a fresh worker UTXO \
                         (attempt {}/{})",
                        worker_idx,
                        hex::encode(worker.transaction.id),
                        worker.index,
                        attempt,
                        SEND_MAX_ATTEMPTS,
                    );
                    self.replace_with_fresh_worker(&mut worker, need).await;
                }
                Err(e) => panic!("Failed to fund recipient from faucet worker: {e:?}"),
            }
        };
        let tx_id_hex = hex::encode(response.transaction.id);

        let dest_utxo = self
            .faucet
            .find_utxo_by_tx_id(address, tx_id_hex.clone())
            .await
            .expect("Destination UTXO never confirmed");

        let faucet_address = self.faucet.address_as_bech32();
        let new_worker = self
            .faucet
            .find_utxo_by_tx_id(&faucet_address, tx_id_hex)
            .await
            .expect("Faucet change UTXO never confirmed");

        tracing::info!(
            "FaucetManager: worker[{}] refreshed {} ADA -> {} ADA (sent {} ADA to {})",
            worker_idx,
            worker.value.lovelace / 1_000_000,
            new_worker.value.lovelace / 1_000_000,
            lovelace / 1_000_000,
            address,
        );
        self.replace_worker_slot(&mut worker, new_worker);
        dest_utxo
    }

    /// Re-query the faucet and swap the slot to a fresh UTXO covering
    /// `need`. Candidate filtering, the pick, and the registry update
    /// happen under one lock acquisition so concurrent recoveries can't
    /// reserve the same candidate. Randomized so re-acquisitions spread
    /// out; panics if nothing suitable is left (genuine top-up situation).
    async fn replace_with_fresh_worker(&self, slot: &mut OgmiosUtxo, need: u64) {
        let utxos = self.faucet.utxos().await;
        let mut assigned = self.assigned.lock().expect("assigned registry poisoned");
        let candidates: Vec<&OgmiosUtxo> = utxos
            .iter()
            .filter(|u| {
                u.value.lovelace >= need.max(MIN_WORKER_LOVELACE)
                    && !assigned.contains(&(u.transaction.id, u.index))
            })
            .collect();
        if candidates.is_empty() {
            panic!(
                "FaucetManager: no live UTXO >= {} lovelace left at {} to replace a stale \
                 worker. Either top up the faucet (send more tADA to this address) or re-prime \
                 it (re-split its balance into fresh large worker UTXOs).",
                need.max(MIN_WORKER_LOVELACE),
                self.faucet.address_as_bech32(),
            );
        }
        let pick = candidates[rand::random::<u32>() as usize % candidates.len()].clone();
        assigned.remove(&(slot.transaction.id, slot.index));
        assigned.insert((pick.transaction.id, pick.index));
        *slot = pick;
    }

    async fn acquire_worker(&self) -> (usize, MutexGuard<'_, OgmiosUtxo>) {
        let start = self.next_worker.fetch_add(1, Ordering::Relaxed) % worker_count();
        for offset in 0..worker_count() {
            let i = (start + offset) % worker_count();
            if let Ok(guard) = self.workers[i].try_lock() {
                return (i, guard);
            }
        }
        let guard = self.workers[start].lock().await;
        (start, guard)
    }

    async fn initialise_workers(faucet: &CardanoClient) -> Vec<OgmiosUtxo> {
        let address = faucet.address_as_bech32();
        let utxos = faucet.utxos().await;
        let total: u64 = utxos.iter().map(|u| u.value.lovelace).sum();

        tracing::info!(
            "FaucetManager: faucet {} holds {} UTXOs, total {} ADA",
            address,
            utxos.len(),
            total / 1_000_000,
        );
        for u in &utxos {
            tracing::info!(
                "  {}#{} -> {} ADA",
                hex::encode(u.transaction.id),
                u.index,
                u.value.lovelace / 1_000_000,
            );
        }

        let mut eligible: Vec<OgmiosUtxo> = utxos
            .iter()
            .filter(|u| u.value.lovelace >= MIN_WORKER_LOVELACE)
            .cloned()
            .collect();
        eligible.sort_by_key(|u| std::cmp::Reverse(u.value.lovelace));

        if eligible.len() >= worker_count() {
            eligible.truncate(worker_count());
            tracing::info!(
                "FaucetManager: {} existing UTXOs >= {} ADA, using as workers (no prime needed)",
                worker_count(),
                MIN_WORKER_LOVELACE / 1_000_000,
            );
            for (i, w) in eligible.iter().enumerate() {
                tracing::info!(
                    "  worker[{}]: {}#{} -> {} ADA",
                    i,
                    hex::encode(w.transaction.id),
                    w.index,
                    w.value.lovelace / 1_000_000,
                );
            }
            return eligible;
        }

        let required = (worker_count() as u64) * MIN_WORKER_LOVELACE + PRIME_FEE_BUFFER;
        if total < required {
            panic!(
                "FaucetManager: faucet has {} ADA total but needs at least {} ADA to prime {} workers \
                 of {} ADA each. Top up {}.",
                total / 1_000_000,
                required / 1_000_000,
                worker_count(),
                MIN_WORKER_LOVELACE / 1_000_000,
                address,
            );
        }

        tracing::info!(
            "FaucetManager: only {} UTXO(s) >= {} ADA, priming {} workers from full balance",
            eligible.len(),
            MIN_WORKER_LOVELACE / 1_000_000,
            worker_count(),
        );
        Self::prime(faucet, &utxos).await
    }

    async fn prime(faucet: &CardanoClient, utxos: &[OgmiosUtxo]) -> Vec<OgmiosUtxo> {
        let address = faucet.address_as_bech32();

        let mut working_utxos = if utxos.len() > MAX_INPUTS_PER_TX {
            tracing::info!(
                "FaucetManager: {} UTXOs exceeds single-tx input limit of {}; pre-consolidating",
                utxos.len(),
                MAX_INPUTS_PER_TX,
            );
            faucet
                .consolidate_utxos(MAX_INPUTS_PER_TX)
                .await
                .expect("Faucet consolidation failed");
            faucet.utxos().await
        } else {
            utxos.to_vec()
        };

        let mut prime_tx_id: Option<[u8; 32]> = None;
        for attempt in 1..=PRIME_MAX_ATTEMPTS {
            let total: u64 = working_utxos.iter().map(|u| u.value.lovelace).sum();
            let required = (worker_count() as u64) * MIN_WORKER_LOVELACE + PRIME_FEE_BUFFER;
            if total < required {
                panic!(
                    "FaucetManager: after dropping ghost UTXOs, faucet has only {} ADA \
                     but needs at least {} ADA to prime {} workers of {} ADA each. Top up {}.",
                    total / 1_000_000,
                    required / 1_000_000,
                    worker_count(),
                    MIN_WORKER_LOVELACE / 1_000_000,
                    address,
                );
            }
            let worker_size = (total - PRIME_FEE_BUFFER) / worker_count() as u64;

            tracing::info!(
                "FaucetManager: prime attempt {}/{}: {} input UTXOs ({} ADA total), \
                 target {} ADA per worker",
                attempt,
                PRIME_MAX_ATTEMPTS,
                working_utxos.len(),
                total / 1_000_000,
                worker_size / 1_000_000,
            );

            match faucet
                .split_to_self(&working_utxos, worker_count(), worker_size)
                .await
            {
                Ok(id) => {
                    prime_tx_id = Some(id);
                    break;
                }
                Err(e) => {
                    let err_str = match &e {
                        OgmiosClientError::RequestError(s) => s.clone(),
                        other => format!("{:?}", other),
                    };
                    let ghosts = extract_unknown_refs(&err_str);
                    if ghosts.is_empty() {
                        panic!("Faucet prime split-tx failed: {:?}", e);
                    }
                    tracing::info!(
                        "FaucetManager: ogmios reported {} ghost UTXO(s); dropping and retrying",
                        ghosts.len(),
                    );
                    for (id, idx) in &ghosts {
                        tracing::info!("  ghost: {}#{}", hex::encode(id), idx);
                    }
                    let before = working_utxos.len();
                    working_utxos.retain(|u| {
                        !ghosts
                            .iter()
                            .any(|(id, idx)| u.transaction.id == *id && u.index == *idx)
                    });
                    if working_utxos.len() == before {
                        panic!(
                            "FaucetManager: ogmios reported ghosts not in our input set: {:?}",
                            ghosts,
                        );
                    }
                }
            }
        }
        let prime_tx_id = prime_tx_id.unwrap_or_else(|| {
            panic!(
                "FaucetManager: prime failed after {} attempts; ghost UTXOs keep appearing",
                PRIME_MAX_ATTEMPTS,
            )
        });
        let prime_tx_id_hex = hex::encode(prime_tx_id);
        tracing::info!("FaucetManager: prime tx submitted: {}", prime_tx_id_hex);

        let outputs = faucet
            .find_utxos_by_tx_id(&address, prime_tx_id_hex.clone())
            .await;

        let mut workers: Vec<OgmiosUtxo> = outputs
            .into_iter()
            .filter(|u| u.value.lovelace >= MIN_WORKER_LOVELACE)
            .collect();
        workers.sort_by_key(|u| std::cmp::Reverse(u.value.lovelace));
        if workers.len() < worker_count() {
            panic!(
                "FaucetManager: prime tx {} produced only {} outputs >= {} ADA, expected {}",
                prime_tx_id_hex,
                workers.len(),
                MIN_WORKER_LOVELACE / 1_000_000,
                worker_count(),
            );
        }
        workers.truncate(worker_count());

        tracing::info!("FaucetManager: priming complete:");
        for (i, w) in workers.iter().enumerate() {
            tracing::info!(
                "  worker[{}]: {}#{} -> {} ADA",
                i,
                hex::encode(w.transaction.id),
                w.index,
                w.value.lovelace / 1_000_000,
            );
        }
        workers
    }
}

/// Extract `(tx_id, index)` pairs from the `unknownOutputReferences` field of an
/// ogmios 3117 error. We narrow the scan to that specific JSON array — other ogmios
/// errors (e.g. 3122 insufficient-fee on Preview) embed the submitted tx's inputs in
/// their data field, which would otherwise be misread as ghosts.
fn extract_unknown_refs(err_str: &str) -> Vec<([u8; 32], u16)> {
    const KEY: &str = "\"unknownOutputReferences\":[";
    let Some(start) = err_str.find(KEY) else {
        return Vec::new();
    };
    let array_start = start + KEY.len();
    let Some(rel_end) = err_str[array_start..].find(']') else {
        return Vec::new();
    };
    let array = &err_str[array_start..array_start + rel_end];

    const ID_TAG: &str = "\"id\":\"";
    const IDX_TAG: &str = "\"index\":";

    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = array[cursor..].find(ID_TAG) {
        let id_start = cursor + rel + ID_TAG.len();
        let Some(id_end_rel) = array[id_start..].find('"') else {
            break;
        };
        let id_end = id_start + id_end_rel;
        let hex_id = &array[id_start..id_end];

        let Some(idx_rel) = array[id_end..].find(IDX_TAG) else {
            break;
        };
        let idx_start = id_end + idx_rel + IDX_TAG.len();
        let idx_end = array[idx_start..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|e| idx_start + e)
            .unwrap_or(array.len());
        let idx_str = &array[idx_start..idx_end];

        cursor = idx_end;

        let Ok(index) = idx_str.parse::<u16>() else {
            continue;
        };
        let mut id = [0u8; 32];
        if hex::decode_to_slice(hex_id, &mut id).is_ok() {
            out.push((id, index));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::extract_unknown_refs;

    #[test]
    fn parses_unknown_output_references() {
        let err = r#"ErrorObject { code: ServerError(3117), message: "...", data: Some(RawValue({"unknownOutputReferences":[{"transaction":{"id":"04af6c5ccc3c828c1c706c576945d72303a8f3ef749794ff34d813acaff8e16c"},"index":1},{"transaction":{"id":"274e21d0de7978852687b0d67256de678ccdd0b039adfbfce11510c10ad78fc1"},"index":0}]})) }"#;
        let refs = extract_unknown_refs(err);
        assert_eq!(refs.len(), 2);
        assert_eq!(
            hex::encode(refs[0].0),
            "04af6c5ccc3c828c1c706c576945d72303a8f3ef749794ff34d813acaff8e16c"
        );
        assert_eq!(refs[0].1, 1);
        assert_eq!(refs[1].1, 0);
    }

    #[test]
    fn returns_empty_on_unrelated_error() {
        let err = r#"RequestError("some other error")"#;
        assert!(extract_unknown_refs(err).is_empty());
    }

    #[test]
    fn ignores_input_refs_outside_unknown_array() {
        // ogmios's 3122 insufficient-fee error can embed the submitted tx's inputs in
        // its data — we must not interpret those as ghosts.
        let err = r#"ErrorObject { code: ServerError(3122), message: "Insufficient fee!", data: Some(RawValue({"minimumRequiredFee":{"ada":{"lovelace":818873}},"providedFee":{"ada":{"lovelace":227805}},"transaction":{"inputs":[{"transaction":{"id":"04af6c5ccc3c828c1c706c576945d72303a8f3ef749794ff34d813acaff8e16c"},"index":1}]}})) }"#;
        assert!(extract_unknown_refs(err).is_empty());
    }
}
