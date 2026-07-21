# Increase local devnet pre-funded wallets from 4 to 20

The `undeployed` network genesis now funds 20 wallets instead of 4, so DApp
developers can test from many end-user perspectives on a local devnet without
manually funding extra wallets. Seeds `wallet-seed-4` through `wallet-seed-19`
use their decimal index as the seed value; the original three sequential seeds
and the Lace test wallet are unchanged. The genesis wallets e2e now verifies
all 20 wallets.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1345
