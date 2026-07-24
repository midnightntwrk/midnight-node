#tests #ci
# Add e2e regression guard for block authorship & committee rotation

Adds an ignored e2e test (`tests/e2e/tests/block_production.rs`) that audits the
newest finalized blocks and asserts the deterministic invariants behind the
aura-fork removal (#1700) and pallet-session migration (#1800):

- every block's AURA author (`aura.authorities.at(parent)[slot % len]`) is a
  member of `sidechain_getEpochCommittee` for the parent's sidechain epoch
  (parent-state read → boundary-safe: the first block of a session is produced
  by the previous committee);
- per epoch, on-chain `aura.authorities` == `sidechain_getEpochCommittee`;
- `session.validators` and `aura.authorities` stay equal length;
- session index is monotonic and finalized heights are contiguous.

Adds the supporting `MidnightClient` helpers (`get_epoch_committee`,
`get_block_digest_info`, `get_aura_authorities_hex_at`, `get_session_index_at`,
`get_session_validators_len_at`, `get_sidechain_epoch_and_slot`,
`get_mainchain_epoch`). The test is `#[ignore]` so the parallel suite skips it;
`+local-env-ci` runs it as the final step (after the suite + health check) so it
analyses the most blocks — bounded to the newest 200.

PR: (added when the PR is opened)
Issue: https://github.com/midnightntwrk/midnight-node/issues/1759
