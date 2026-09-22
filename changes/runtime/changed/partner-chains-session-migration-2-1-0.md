#runtime #partner-chains
# Migrate to upstream pallet_session with automatic key registration (2.1.0)

Integrates upstream Partner Chains changes for automatic session key
registration (#1026) and removal of `pallet-partner-chains-session` (#1025),
adapted for midnight-node.

Partner Chains runtimes now use stock Substrate `pallet_session` directly.
`pallet_session_validator_management` (`SessionCommitteeManagement`) implements
`SessionManager` and `ShouldEndSession` inline — the old `PalletSessionSupport`
wrapper and `pallet-partner-chains-session` crate are removed. Committee members'
session keys are registered automatically at genesis and on each committee
rotation via `pallet_session::SessionInterface::set_keys`, so block producers no
longer need to call `set_keys` manually.

Midnight-node-specific wiring:

- Runtime migrates off `pallet-partner-chains-session` to a direct
  `pallet_session::Config` impl delegated to `SessionCommitteeManagement`.
- `pallet_session::historical` is enabled and wired (`Historical` pallet +
  `historical::Config`) so privileged `SessionInterface::set_keys` works under
  the SDK's ownership-proof checks (dispatching the `set_keys` extrinsic with
  an empty proof silently fails).
- `pallet_session` extrinsics are disabled via `#[runtime::disable_call]`; key
  registration is internal only.
- Genesis chain spec no longer duplicates session keys — registration happens
  through committee management genesis.
- The local `session_manager` module (`ValidatorManagementSessionManager`) is
  removed.
- `spec_version` bumped to `002_001_000`; metadata regenerated as
  `midnight_metadata_2.1.0.scale`.

Requires a metadata rebuild and a runtime upgrade on live chains.

PR: https://github.com/midnightntwrk/midnight-node/pull/1800
Issue: https://github.com/midnightntwrk/midnight-node/issues/1759
