#client
# Remove costly call to `gc()` until it's optimised

We were calling the Ledger storage garbage collection synchronously at the end of each block. After running some benchmarks, we've found that it's in need of optimisation. Removing for now.

PR: https://github.com/midnightntwrk/midnight-node/pull/750/changes
Possible cause for: https://shielded.atlassian.net/browse/PM-21969
