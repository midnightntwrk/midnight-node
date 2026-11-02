# Test authorities with real Cardano observations

`USE_MAIN_CHAIN_FOLLOWER_MOCK` already selects the node's mock Cardano data
sources. For isolated test chains that need real Cardano observations, set:

```sh
USE_MAIN_CHAIN_FOLLOWER_MOCK=authorities
MOCK_REGISTRATIONS_FILE=/path/to/test-registrations.json
```

Supply the normal db-sync connection, TLS and Cardano timing configuration.
The node reuses the existing validator-registration and federated-governance
mocks, while block references, sidechain RPC, cNight observations and bridge
transfers continue to use db-sync.

| Setting | Observations | Authorities |
|---|---|---|
| `false` (default) | db-sync | db-sync |
| `true` | Existing mocks | Existing test mocks |
| `authorities` | db-sync | Existing test mocks |

Existing boolean configuration values and environment strings `true` and
`false` retain their behavior. For TOML, the new value is the quoted string
`use_main_chain_follower_mock = "authorities"`. Unknown values are rejected.
Use the same configuration on every node participating in the test chain.
This option uses known test keys and is intended only for isolated test chains.

## Starting from a real-network snapshot

This setting alone cannot start an untouched snapshot with different validator
keys. The snapshot already contains its current committee and consensus
state. Changing the candidate source does not replace that committee: an
existing authority must produce blocks before a new committee can take over.

The clone preparation and node configuration therefore serve different roles:

1. Before starting validators, prepare an isolated copy of the snapshot with
   the test committee, matching consensus/finality state and local keystores.
   Keep the snapshot's genuine Cardano reference and observation positions.
2. Start every clone node from that same prepared state, using
   `USE_MAIN_CHAIN_FOLLOWER_MOCK=authorities` and matching test registrations.
   The prepared committee can produce the first blocks; the mock candidate
   source supplies test keys for subsequent committee selection.
3. Verify advancing finality and real Cardano observations across a committee
   transition. Successful startup alone does not establish this.

The `mock-authorities` converter in `shieldedtech/midnight-node-ops` performs
snapshot authority replacement. Its real-Cardano conversion support preserves
source references rather than substituting synthetic ones. Check the
converter's supported snapshot consensus and storage layouts before use.
The node setting performs neither snapshot conversion nor consensus-state
repair. Existing database permissions and query prerequisites still apply.
