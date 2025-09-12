# Fix `claim-rewards` command in `midnight-node-toolkit`

The toolkit's `claim-rewards` method was not working correctly. Correct a fee calculation issue introduced when DUST was enabled. Also, correctly generate separate nonces for each claim.

PR: https://github.com/midnightntwrk/midnight-node/pull/1163
