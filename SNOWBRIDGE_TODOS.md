# Snowbridge work beyond the local PoC

The [recorded bidirectional run](tests/snowbridge/evidence/run-mjrg6180.validation.json)
passed with real SP1 proofs, committee-state checks, Compact token use,
authenticated failure/retry and completed Ethereum payout. See the
[runbook](tests/snowbridge/README.md), [SP1 trust model](tests/snowbridge/sp1/README.md)
and [evidence provenance](tests/snowbridge/evidence/README.md).

Remaining work before a supported bridge:

- Benchmark runtime weights, proof bounds, committee-scale proving cost and latency.
- Publish compatible ledger/client dependencies and validate existing-chain migration.
- Validate independent indexer replay and cold recovery.
- Exercise live validator membership/count changes and authority-boundary recovery.
- Test full orchestrator restart/reorg recovery and implement durable relayer services.
- Authenticate production bootstrap checkpoints, use independent source connections
  and define a source-freshness policy. Committee checks cannot detect an isolated
  relayer being denied newer blocks or shown an older valid prefix.
- Generalize beyond one asset, one authority and one outbound message per block.
- Define deployment, upgrade, emergency-pause, custody and audit requirements.

The [production work breakdown](docs/proposals/snowbridge-sp1-production-work-breakdown.md)
contains the detailed acceptance criteria and dependencies. None of these
production gates is implied by the local acceptance result.
