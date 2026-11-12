#node #consensus #babe #migration

# Check a BABE block against its parent before verification

A block routed to the BABE pipeline must follow its parent by number, and its parent must carry a
readable BABE pre-runtime digest. The import-queue dispatch checks both before the BABE verifier
runs, and rejects a block whose parent is not imported unless the block is imported with its
state.

PR: https://github.com/midnightntwrk/midnight-node/pull/2407
