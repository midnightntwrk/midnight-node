# Update Ledger to 7.0.0-alpha.1

Updates midnight-node to be compatible with midnight-ledger version 7.0.0-alpha.1, which introduces breaking proof system changes:

- `Output::new` and `Output::new_contract_owned` now take `Option<u16>` for segment ID instead of `u16`
- `PreTranscript` struct fields changed from references to owned values (`QueryContext`, `Vec<Op>`)
- `ContractOperationVersion::V2` replaced by `ContractOperationVersion::V3`
- `ContractOperationVersionedVerifierKey::V2` replaced by `V3`

The `ledger/helpers` crate now isolates ledger 7.x-specific modules (`merkle_tree`, `maintenance`) in a `latest_only` directory to maintain compatibility with the hard-fork test version (6.2.0-rc.2).

Jira: https://shielded.atlassian.net/browse/PM-21895

PR: https://github.com/midnightntwrk/midnight-node/pull/415
