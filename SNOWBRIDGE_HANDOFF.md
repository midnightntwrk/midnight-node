# Snowbridge bridge PoC handoff

Start with the [local runbook](tests/snowbridge/README.md#local-end-to-end-runbook),
[component diagrams](tests/snowbridge/ARCHITECTURE.md) and
[SP1 runbook](tests/snowbridge/sp1/README.md). The source, dependency patches and
build scripts in this branch implement a local two-way Ethereum–Midnight token
bridge. The [manual walkthrough](tests/snowbridge/MANUAL.md) adds operator-paced
Polkadot.js submissions and a read-only Compact/Ethereum state viewer.

## Recorded validation

The [26 October acceptance report](tests/snowbridge/evidence/run-mjrg6180.validation.json)
records `completeBridgeE2E`, `sp1FinalityProved` and `committeeStateVerified` as true,
with redemption `Completed`. The run deposited 100 tokens, issued native tokens,
used them in Compact, retired 40, authenticated an intentional Ethereum release
failure, retried, paid 40 and authenticated completion on Midnight. Both real SP1
submissions finalized, including a session rotation with unchanged membership.

The guest checks each selected commitment against its runtime MMR tip, while
pinned current/next committees constrain rotation. The relayer defers the rotation
block until a later commitment confirms the successor. This protects committee
integrity and ordering; source freshness and availability remain operational concerns.

Earlier milestones are preserved in the [25 October SP1 report](tests/snowbridge/evidence/run-oiulvrxd.validation.json)
and [23 October direct-signature report](tests/snowbridge/evidence/run-fllxkhdn.validation.json).
The [evidence guide](tests/snowbridge/evidence/README.md) explains provenance,
hashes, omitted raw artifacts and the small public real-proof fixture. These are
historical local runs, not validation of a newly built commit.

## Scope and continuation

Use fresh genesis, one conventional ERC-20, one Midnight authority, at most 64
deposits/retirements, four release attempts and one outbound message per block.
The runtime feature `ethereum-bridge-poc` requires the explicit patched-ledger
build described in the runbook. Existing-chain migration is unsupported.

Remaining work is tracked in [SNOWBRIDGE_TODOS.md](SNOWBRIDGE_TODOS.md).
The [production architecture](docs/proposals/snowbridge-sp1-production-architecture.md)
and [work breakdown](docs/proposals/snowbridge-sp1-production-work-breakdown.md)
separate implemented behavior from proposed production work.

The original [runtime/ledger research spike](docs/proposals/snowbridge-runtime-ledger-integration-spike.md),
[implementation specification](docs/proposals/snowbridge-implementation-spec.md),
[Ethereum contracts spike](docs/proposals/snowbridge-ethereum-contracts-spike.md),
[Compact usability spike](docs/proposals/snowbridge-compact-token-usability-spike.md)
and [testing strategy](docs/proposals/snowbridge-e2e-testing-strategy.md) remain
design history. Their early statements about missing implementation describe
that research stage; use the runbooks for current behavior.

Raw run evidence, dependency checkouts, databases and binaries under `artifacts/`
are ignored. Preserve an existing run before recreating its dependencies or
moving machines. The tracked reports and proof fixture are a compact review
record, not a complete operational recovery bundle.
