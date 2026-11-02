#toolkit #ledger #unshielded #ecdsa

# Toolkit ECDSA unshielded signature support

From ledger 9 the ledger natively supports a second unshielded (NIGHT) signature scheme, ECDSA,
alongside Schnorr. The toolkit can now generate and spend with ECDSA identities while keeping
Schnorr the default.

- `ledger/helpers` exposes per-version `SigningKeyEcdsa`/`VerifyingKeyEcdsa` types and `*_ecdsa`
  adapters. ECDSA is real on ledger 9 (`base_crypto::ecdsa`) and stubbed (`unimplemented!`) on
  ledger 7/8, so the shared `common` code compiles against all three generations but only
  functions on ledger 9.
- `UnshieldedWallet` now stores a scheme enum (`UnshieldedWalletKeys::{Schnorr, Ecdsa}`) behind
  scheme-agnostic methods (`verifying_key`/`transaction_signing_key`/`sign`) that return the
  ledger-version signature types, so downstream builders never see a raw per-scheme key. The
  persisted layout changed, so the tag is bumped to `unshielded-wallet[v2]`.
- HD `Role::Metadata` (index 4) is repurposed as `Role::Ecdsa` (`m/44'/2400'/0'/4/0`).
- CLI: every seed-bearing flag now takes a single value with an optional scheme prefix —
  `--seed <seed>` (bare, defaults to Schnorr, backwards compatible), `--seed schnorr:<seed>`
  (explicit Schnorr), or `--seed ecdsa:<seed>` (ECDSA). This applies uniformly to every command
  whose seed drives a NIGHT signature:
  - `show-address`, `show-wallet`, `show-seed`, `dust-balance` (for `show-seed` the output is the
    raw, scheme-independent seed bytes, so the prefix only affects parity and is a no-op there).
  - `generate-txs single-tx` (`--source-seed`, `--funding-seed`), `register-dust-address` /
    `deregister-dust-address` (`--wallet-seed`, `--funding-seed`), `claim-rewards` and `batches`
    (`--funding-seed`).
  - `transfer` / `batch-single-tx` JSON transfer specs accept a `source_seed` string and an
    optional `funding_seed` string, each parsed with the same `schnorr:`/`ecdsa:` prefix rule.
  - The chosen per-seed scheme is threaded into the fork-aware context/cache builders, so a NIGHT
    identity is built, resolved and signed for with the requested scheme.
  - `generate-genesis` stays Schnorr-only (its seeds come from a JSON `--seeds-file`); contract
    deploy/maintenance committees also stay Schnorr.
- The fetch wallet-state cache is versioned (`WALLET_CACHE_FORMAT_VERSION = 2`) and its key folds
  in the signature scheme, so Schnorr and ECDSA identities for one seed no longer collide and
  pre-ECDSA cache entries are invalidated and evicted.
- ECDSA is rejected with a clear error on pre-ledger-9 fork paths.

PR: https://github.com/midnightntwrk/midnight-node/pull/1837
Issue: https://github.com/midnightntwrk/midnight-node/issues/1542
