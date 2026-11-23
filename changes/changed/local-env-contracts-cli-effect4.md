# local-env deploys contracts with the reserve contracts CLI rewrite

The local-env contract compiler runs the rewritten midnight-reserve-contracts CLI (`vpx cli/main.ts` under pnpm) instead of `bun cli`. `res/local` picks up the bridge and reserve transaction hashes that the new CLI's deploy produces.

PR: https://github.com/midnightntwrk/midnight-node/pull/2508
