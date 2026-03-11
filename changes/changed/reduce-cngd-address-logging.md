#node
# Reduce cNIGHT observation address logging level

Downgrade non-bech32 and no-delegation-part Cardano address logs from error to debug in cNIGHT observation data source, as these are expected for certain address types.

PR: https://github.com/midnightntwrk/midnight-node/pull/905