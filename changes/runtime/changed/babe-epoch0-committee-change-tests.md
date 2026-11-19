#runtime #consensus #babe #migration #tests
# Test BABE epoch 0 across committee changes at the flip

The across-the-flip observer test now runs one scenario per committee change: reordered, growing,
replaced and re-seated (D-parameter) committees, failed selection, an aborted hand-off, a postponed
flip and a halt with the flip scheduled. Each pins the outgoing committee and BABE epoch 0's
authorities. Tests only; no runtime change.

PR: https://github.com/midnightntwrk/midnight-node/pull/2315
Issue: https://github.com/midnightntwrk/midnight-node/issues/1740
