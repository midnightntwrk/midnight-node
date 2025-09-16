# Midnight Relayer

1. Start node with the following args:
```
        --state-pruning archive
        --blocks-pruning archive
        --enable-offchain-indexing true
```

2. Ensure that BEEFY begins. The node must have relevant BEEFY keys inserted, and the first session must have passed.

Note that for running the local node, you may need to insert the BEEFY key manually, after the node starts, if BEEFY does not start on its own. 