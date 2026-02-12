#node #breaking

# Migrate to Aiken-based Permissioned Candidates Contracts

Update permissioned candidates policy IDs to use new Aiken-based contracts instead of the deprecated Haskell-based contracts.

**Policy ID Updates:**
- node-dev-01: `51f812332ccc276d1dfa9da923c2235b91a5150ff275b633a5fa1bdb`
- qanet: `6c327f1fe5e3b2619c62ca642892146c7326a91dc47f6006f6cdf690`
- preview: `4057188de00d74c6679263989745309f02bf55f8806061943124489b`
- preprod: `369ee95be4c68a2984733a8c727ecd28df3039a3e5f1e80290b08eec`

**Breaking Change**: Nodes must use the updated policy IDs to correctly read permissioned candidates from Cardano. The Aiken contracts use a different data format:
- `sidechainPublicKey`: hex string
- `keys`: object with `aura` and `gran` keys
- `isValid`: boolean

**Local Environment Updates:**
- Added dynamic Aiken contract compilation for local-env E2E tests
- Added federated_ops contract deployment support
- Governance contracts are now compiled from source during local-env startup

PR: https://github.com/midnightntwrk/midnight-node/pull/454
JIRA: https://shielded.atlassian.net/browse/PM-20994
