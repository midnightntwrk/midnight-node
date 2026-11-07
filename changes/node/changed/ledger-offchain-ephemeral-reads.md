#node #ledger
# Run off-chain ledger reads against the ephemeral ledger storage

Ledger reads outside block authoring and block import now use the ephemeral ledger
storage, like the other ledger host calls there. Reads allocate, and in the shared
storage a later flush could write those allocations to disk. Return values are
unchanged, and no runtime upgrade is required.

