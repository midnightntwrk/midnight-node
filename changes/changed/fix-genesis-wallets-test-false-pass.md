# Genesis wallets test: correct NODE_CONTAINER default and fail on toolkit errors

`scripts/genesis_wallets_test.sh` now defaults `NODE_CONTAINER` correctly
(the fallback previously assigned `NETWORK`, leaving the node URL empty),
treats a failing or report-less toolkit invocation as a test failure instead
of silently counting the seed as funded, matches `show-wallet`'s default JSON
output (the previous debug-format grep could never match, so the check was a
no-op), and checks the actual four funded genesis wallets — the old list
included `0x..04`, which has never been funded, masked by the false pass.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1905

PR: https://github.com/midnightntwrk/midnight-node/pull/1916
