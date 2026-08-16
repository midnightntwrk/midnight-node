#runtime #governance
# Enter safe mode instead of freezing the chain on a failed migration

Previously a failed multi-block migration used
`frame_support::migrations::FreezeChainOnFailedMigration`, which leaves the migration
cursor `Stuck` and forces the executive into `OnlyInherents` mode forever. That blocks
every extrinsic — including governance and `System::set_code` — so the chain can only
be recovered off-chain (a coordinated hard fork). The polkadot-sdk docs themselves
call this "not a sane default, since it prevents governance intervention".

The runtime now uses a governance-recoverable safe mode instead
(`pallet_midnight_system::EnterSafeModeOnFailedMigration`). On a migration failure it
records the failure, enters safe mode, and returns `ForceUnstuck` so the chain keeps
producing blocks and accepting extrinsics. While safe mode is active, the base call
filter (`pallet_midnight_system::SafeModeFilter`, composed with the existing
`TxPause`) blocks all non-governance transactions so the inconsistent state cannot be
touched, while whitelisted governance calls still go through — letting governance
repair state and/or ship a corrected runtime on-chain, then call
`MidnightSystem::exit_safe_mode`. Inherents (`DispatchClass::Mandatory`) are always
allowed so block production never stalls, and the safe-mode whitelist reuses the same
`GovernanceAuthorityCallFilter` the always-on `CheckCallFilter` enforces, so the two
cannot drift apart.

This is a runtime behavior change; `spec_version` is bumped and metadata rebuilt.

PR: <link to PR>
Issue: <link to Github Issue, if applicable>
