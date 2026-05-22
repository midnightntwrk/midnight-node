#node #runtime #partner-chains
# Align node, runtime, relay, and partner-chains with polkadot-stable2603 SDK

Bumps Substrate dependencies to the `polkadot-stable2603` tag and updates call sites for breaking API changes:

- **Workspace:** All `polkadot-stable2512-3` git deps moved to `polkadot-stable2603`; `tracing-subscriber` pinned to `=0.3.19` (required by `sp-tracing` on this line) with toolkit using the workspace entry.
- **Node:** `sc_service::build_network` gains `spawn_essential_handle`; `new_full_parts_with_genesis_builder` keeps the six-argument signature (no Grandpa pruning filters argument—unlike `new_full_parts`).
- **Runtime:** `sp_session::SessionKeys::generate_session_keys` now takes `owner: Vec<u8>` and returns `OpaqueGeneratedSessionKeys`; opaque key `generate` calls pass `&owner`.
- **Partner-chains (vendored subtree):** Aura `Proposer` uses `ProposeArgs`; demo node uses `GrandpaPruningFilter` with `new_full_parts` and `spawn_essential_handle`; toolkit inherent errors use `Debug` instead of `sp_runtime::RuntimeDebug` where needed.
- **Ledger / primitives:** `RuntimeDebug` derives replaced with `core::fmt::Debug` where `sp_runtime::RuntimeDebug` / `frame_support::RuntimeDebug` were removed.
- **Pallets:** `RuntimeDebugNoBound` replaced with `DebugNoBound` (e.g. federated-authority, throttle).
- **Relay (BEEFY):** `BeefySignatureHasher` removed; `SignedCommitment::verify_signatures` called with a single inferred authority type parameter.

Partner-chains `Cargo.toml` / README / changelog are aligned with the same SDK tag where applicable.

PR: https://github.com/midnightntwrk/midnight-node/pull/1299
Required for https://github.com/midnightntwrk/midnight-node/issues/1245
