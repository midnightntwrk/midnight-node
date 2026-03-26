#node #genesis
# Improve genesis contruction and verification

- Genesis construction script (`genesis-construction.sh`) with interactive wizard supporting skippable verification steps, genesis messages, and fee checking
- Fixed genesis query bugs: `policy_id` decoding, asset name encoding, SQL amount casting to `BIGINT`
- UTXO filtering in cnight genesis to exclude UTXOs without a prior registration
- Enabled all verification steps for mainnet genesis
- Added genesis message
- Verify ledger fees
- Add bootnodes as a congif file
- Improve genesis generarion addid `--no-cache` to Earthly commands

PR: https://github.com/midnightntwrk/midnight-node/pull/694
JIRA: https://shielded.atlassian.net/browse/PM-20554
