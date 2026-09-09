#node

# Make committee membership probe engine-agnostic

The committee-membership watcher no longer decodes `CurrentCommittee` /
`SessionKeys` from raw storage. It uses `ConsensusEngineApi::active_engine` to
choose `AuraApi::authorities` or `BabeApi::current_epoch`, so the same task
covers AURA and BABE without a SessionKeys layout change.

A runtime older than `ConsensusEngineApi` is treated as AURA.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
PR:
