#runtime
# Use generated benchmark weights in runtime

Connects generated runtime benchmark weights so the runtime uses measured
weights for core FRAME and local runtime pallets instead of generic defaults.
Includes GRANDPA, BEEFY MMR, timestamp, migrations, scheduler, preimage, tx-pause, council and technical committee
collectives and memberships, session validator management, federated authority
and observation, system parameters, and cNIGHT observation.

PR: https://github.com/midnightntwrk/midnight-node/pull/1482
Issue: https://github.com/midnightntwrk/midnight-node/issues/1450