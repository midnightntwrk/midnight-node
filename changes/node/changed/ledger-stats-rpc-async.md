#node #rpc #ledger

# Serve `midnight_ledgerStats` off the async executor

`midnight_ledgerStats` was a synchronous handler that held a mutex across the
whole arena read. The read is O(1) in state size but not in latency: it waits on
the arena's process-global locks and loads root nodes from disk. Run inline on a
tokio worker, with every concurrent caller parked on one lock, a burst of calls
could stall the shared executor that networking and consensus gossip also run on.

The handler is now async and hands the arena read to the blocking pool. The memo
lock is only held for the lookup and the store. Concurrent callers for the same
block still share a single read, so a polling burst on a fresh block touches the
arena once, but the single-flight is per block hash rather than one global lock,
so callers for different blocks proceed independently.

PR: https://github.com/midnightntwrk/midnight-node/pull/
