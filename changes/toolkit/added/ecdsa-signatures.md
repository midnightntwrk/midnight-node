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
- CLI: `--seed` selects Schnorr, `--seed-ecdsa` selects ECDSA (mutually exclusive). `show-address`
  derives the unshielded address, verifying key and user address for the chosen scheme.
- The fetch wallet-state cache is versioned (`WALLET_CACHE_FORMAT_VERSION = 2`) and its key folds
  in the signature scheme, so Schnorr and ECDSA identities for one seed no longer collide and
  pre-ECDSA cache entries are invalidated and evicted.
- ECDSA is rejected with a clear error on pre-ledger-9 fork paths.

PR: <link to PR>
