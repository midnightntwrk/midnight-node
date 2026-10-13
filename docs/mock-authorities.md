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

This selects data sources; it does not convert a snapshot or rewrite consensus
state. Clone tooling must separately install matching test authorities and
preserve valid Cardano references and observation positions for the selected
network. Existing database permissions and query prerequisites still apply.
