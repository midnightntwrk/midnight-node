# Real Cardano observations for ephemeral chains

Set `MAIN_CHAIN_FOLLOWER_MODE=ephemeral` and
`USE_MAIN_CHAIN_FOLLOWER_MOCK=false` when creating a private fork that follows
its source chain’s Cardano network. This mode reads block references, sidechain
RPC data, cNight observations, and bridge transfers from db-sync, while keeping
validator selection and federated governance on the local mock sources.

Provide `MOCK_REGISTRATIONS_FILE`, `DB_SYNC_POSTGRES_CONNECTION_STRING`,
`CARDANO_SECURITY_PARAMETER`, `CARDANO_ACTIVE_SLOTS_COEFF`,
`BLOCK_STABILITY_MARGIN`, and the source network’s `MC__*` timing parameters.
Use a read-only database role and `SSL_ROOT_CERT` for certificate validation.
Database indexes must be provisioned by the database owner; ephemeral mode
skips startup index creation and cNight query-helper maintenance DDL. Each of
its four observation pools is capped at two connections (eight per node,
56 for seven validators, or 64 including RPC). Query-helper processes also
use a two-connection cNight pool.

The fork converter must preserve the source snapshot’s real Cardano block
reference and observation positions. A clone already running with synthetic
Cardano blocks cannot be switched to this mode. The restored contract addresses,
timing parameters, and db-sync database must belong to the same Cardano network.
All validators and RPC nodes must use the same mode.

Omitting `MAIN_CHAIN_FOLLOWER_MODE` preserves the existing boolean selection.
Explicit `mock` requires `USE_MAIN_CHAIN_FOLLOWER_MOCK=true`; explicit `db-sync`
and `ephemeral` require it to be false. Contradictions fail configuration
validation. Database errors never fall back to mocked observations.

Full node service initialization checks that db-sync has a fresh
tip and a stable block eligible at the current time, the restored non-genesis
header’s Cardano reference resolves, and the cNight cursor’s hash and block number
match db-sync. Only the complete all-zero
uninitialized cursor is exempt. Failed checks stop startup before block import
or authoring. Offline commands such as `chain-info` do not perform this check;
preparation must start the converted node before snapshot fan-out. The tip
freshness check is a conservative heuristic: a genuine Cardano production stall
can also prevent a clone from starting.

Images supporting this contract ship `/res/ephemeral-cardano-v1.json` with
`{"version":1,"mode":"ephemeral"}`. Clone tooling should reject images without
this marker before publishing configuration. A successful test must demonstrate
real Cardano hash resolution and common finality across a session transition,
with local validator and governance keys retained.
