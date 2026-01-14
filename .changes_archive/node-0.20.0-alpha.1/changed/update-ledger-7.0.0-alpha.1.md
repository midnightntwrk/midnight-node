# Update Ledger to 7.0.0-alpha.1

Updates midnight-node to be compatible with midnight-ledger version 7.0.0-alpha.1, which introduces breaking proof system changes:

- `Output::new` and `Output::new_contract_owned` now take `Option<u16>` for segment ID instead of `u16`
- `PreTranscript` struct fields changed from references to owned values (`QueryContext`, `Vec<Op>`)
- `ContractOperationVersion::V2` replaced by `ContractOperationVersion::V3`
- `ContractOperationVersionedVerifierKey::V2` replaced by `V3`

Both the latest and hard-fork test versions now use ledger 7.0.0-alpha.1, so the `merkle_tree` and `maintenance` modules have been unified in `ledger/helpers/src/versions/common/contract/`.

Jira: https://shielded.atlassian.net/browse/PM-21895

PR: https://github.com/midnightntwrk/midnight-node/pull/415
