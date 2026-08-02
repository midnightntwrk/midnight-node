#node #fix
# Confirm governance runtime upgrades by spec_version, not by decoding the upgrade block's events

`federatedRuntimeUpgrade` in `local-environment` previously decided success by
decoding the applying block's events and asserting `System.CodeUpdated` was
present. At an upgrade block the events are encoded by the parent (pre-upgrade)
runtime, while clients resolve metadata at the block's own (post-upgrade) hash;
when the event shape changes across the upgrade (as it does at the ledger v8->v9
hardfork, where `CodeUpdated` gains a hash field) that decode fails and the event
reads as absent. The tool then reported a hard failure on an upgrade that had
actually executed, which is intermittent (a race on when the client refreshes
metadata) and misleading enough that an operator might retry a successful upgrade.

The success check now confirms the upgrade by reading the runtime `spec_version`
before applying and at the applied block via `state_getRuntimeVersion` (which
decodes `Core_version`, not the events storage, so it is immune to the mismatch),
and treats a changed `spec_version` as success. It falls back to the
`System.CodeUpdated` event only when `spec_version` is unchanged (the
`--allow-same-version` path, where the event shape cannot have changed).

Issue: https://github.com/midnightntwrk/midnight-node/issues/1960
