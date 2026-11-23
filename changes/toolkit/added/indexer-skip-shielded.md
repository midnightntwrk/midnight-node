#toolkit #indexer
# Skip the shielded sync on the indexer path

`--indexer-skip-shielded` (or `MN_INDEXER_SKIP_SHIELDED`) makes an `--indexer-url` wallet sync skip
the shielded stream and treat each wallet as holding no shielded coins. Unshielded NIGHT and DUST
still sync as before.

The indexer checks every transaction on the chain for a wallet it has not seen before, starting at
the first one. On a chain with millions of transactions that takes hours per wallet, so commands
that only move NIGHT or spend DUST waited on a sync whose result they never use. A cached shielded
state is kept as it was, so a later run without the flag resumes from it.
