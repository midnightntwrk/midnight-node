#c2m-bridge

# Fix unnecessary denomination in C-to-M bridge

The c2m-bridge pallet incorrectly assumed that amount handed by partner-chains bridge are NIGHT token.
The amounts that are recorded on Cardano are STAR.
Ledger operates on STAR as well, so there is no denomination required.

PR: https://github.com/midnightntwrk/midnight-node/pull/1608
Issue: https://github.com/midnightntwrk/midnight-node/issues/1607
