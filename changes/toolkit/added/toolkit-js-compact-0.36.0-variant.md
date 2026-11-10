#toolkit

# Support compactc 0.36.0 in toolkit-js

Adds the `compact-0.36.0` variant workspace, pinning
`@midnight-ntwrk/compact-js*` 3.0.0 and `@midnight-ntwrk/compact-runtime`
0.21.0, so `midnight-node-toolkit-js` can drive contracts compiled with
`compactc` 0.36.0, which `COMPACTC_VERSION` now defaults to.

compactc 0.36 emits only ZKIR v3, so the toolkit's local ledger-9 prover now
proves v3 circuits with `midnight-zkir` 3.x and keeps v2 circuits on the
existing 2.x prover.

PR: https://github.com/midnightntwrk/midnight-node/pull/2402
