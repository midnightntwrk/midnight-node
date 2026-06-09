#node #tests
# Decouple qanet e2e tests from local-env docker artefacts

`tests/e2e/src/config.rs` always read `contracts-info.json` /
`plutus-local.json` from
`local-environment/src/networks/local-env/runtime-values/`, which only
exists after `docker compose up` in local-env has been run on the host.
The qanet nightly job, after switching to a clean container runner,
started panicking at `Settings::default()` because the directory was
absent.

The qanet feature now points at the `midnight-reserve-contracts`
submodule's qanet deployment snapshot
(`deployments/qanet/contract-info.json` +
`deployed-scripts/qanet/plutus.json`), which is checked in alongside the
submodule pin. `local-*` features still read the docker-generated
runtime-values dir, and `RUNTIME_VALUES_DIR` continues to work as the
historic per-dir override.

`mapping_validator_address` and `mapping_validator_policy_id` now derive
from the compiled validator script (same pattern as
`cnight_token_policy_id`), because the upstream qanet `contract-info.json`
has no `cNIGHT Generates Dust` entry. Both features compute the same
values they did before.

PR: https://github.com/midnightntwrk/midnight-node/pull/1666
Issue: https://github.com/midnightntwrk/midnight-node/issues/1609
