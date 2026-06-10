#node #local-env
# Patch cnight mapping_validator_address from freshly compiled contracts

The midnight-setup entrypoint built the runtime `cnight-config.json` directly
from `res/local-environment/cnight-config.json`, which hardcodes the mapping
validator address. Without this fix, bumping the contracts submodule changes
the deployed address but the runtime keeps the stale one, and cnight tests
fail.

The entrypoint now extracts the `cNIGHT Generates Dust` address from
`/runtime-values/contracts-info.json` and patches it into the cnight config
before chain-spec generation, mirroring how council / tech-auth /
federated-ops policy IDs and addresses are already patched in.

PR: https://github.com/midnightntwrk/midnight-node/pull/1653
Issue: https://github.com/midnightntwrk/midnight-node/issues/1397
