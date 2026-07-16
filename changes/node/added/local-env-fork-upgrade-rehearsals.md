# Add local fork-testing support for the 1.0.0 release train

Extends `local-environment/` with the changes needed to rehearse fork-testing
for the 1.0.0 release train against forked `preview`, `preprod`, and
`mainnet` state. The new well-known network configs restore a remote snapshot,
convert validator state with `mock-authorities`, and bring the fork up in a
fully local mock-authority mode.

Adds a two-phase `full-upgrade` command that rolls node images and then runs
the governance runtime-upgrade flow against the live fork, plus an
`--allow-same-version` escape hatch for rehearsals where the candidate runtime
intentionally keeps the same `spec_version`.

Subsequent runs can now reuse previously restored local snapshot state, with
guardrails that verify the generated mock-authorities artifacts and restored
`data/` directories are still present before reusing them.

PR: https://github.com/midnightntwrk/midnight-node/pull/1522
Issue: https://github.com/midnightntwrk/midnight-node/issues/1468
