#toolkit #refactor
# Abstract toolkit transaction builders over a `BuilderContext` trait

Transaction builders in `ledger/helpers` and `util/toolkit` previously took a
concrete `Arc<LedgerContext<D>>`, which forced every builder to replay the whole
chain locally. They are now generic over a new `BuilderContext<D>` trait that
exposes only the queries builders actually need (latest block context, ledger
parameters, network id, unshielded UTXOs, zswap state, contract state, resolver,
well-formedness check, and wallet access). `LedgerContext` implements the trait,
so runtime behaviour is unchanged. A stub `IndexerContext` proves the trait can
be satisfied by a non-local backend, preparing the toolkit to talk to a node via
indexer queries (issue #1186) without touching the builders again.

No user-visible behaviour change. The `batches` builder retains its previous CLI
surface but stays pinned to the concrete `LedgerContext` backend (it advances
the local ledger between chained transactions, which the abstract trait
deliberately does not expose). It also picks up a new `--coin-selection` flag
matching the other builders.

PR: https://github.com/midnightntwrk/midnight-node/pull/1605
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
