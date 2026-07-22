# Genesis wallets test: correct NODE_CONTAINER default and fail on toolkit errors

`scripts/genesis_wallets_test.sh` now defaults `NODE_CONTAINER` correctly
(the fallback previously assigned `NETWORK`, leaving the node URL empty) and
treats a failing or report-less toolkit invocation as a test failure instead
of silently counting the seed as funded.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1905

PR: https://github.com/midnightntwrk/midnight-node/pull/1916
