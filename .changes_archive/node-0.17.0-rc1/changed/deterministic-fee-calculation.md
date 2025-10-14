# Determistically compute fees in `midnight-node-toolkit`

The size of a proof (and thus, the fee for a TX) is slightly random. We were using different random values when estimating and when paying fees, which could cause us to slightly underpay for transactions. Fix it to always make the same RNG calls in the same order when computing fees and when building the final TX.

PR: https://github.com/midnightntwrk/midnight-node/pull/56