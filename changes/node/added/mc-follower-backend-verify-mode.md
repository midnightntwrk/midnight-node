#node
# Main-chain follower backend selection with embedded indexer + shadow-verify mode

Add `main_chain_follower_backend` config (default `DbSync`, unchanged
behaviour) with two new variants, plus an in-process Cardano indexer to back
them:

- `DbSyncEmbeddedVerify` — answers every data-source query from db-sync
  exactly as today (consensus behaviour bit-identical), while replaying the
  query off the hot path against an **embedded acropolis Cardano indexer**
  running inside the node process, and comparing answers. Outcomes are
  counted in `midnight_mc_follower_verify_total{method,outcome}`; divergences
  on block/epoch-anchored queries are logged at ERROR. Tip-relative
  mismatches, shadow lag, and known-benign field differences (the unused
  Ariadne d-parameter) are counted separately, never as divergence.
  `mc_follower_on_divergence = Halt` exits the node on divergence, for soak
  tests that must fail loudly.
- `Embedded` — reserved for running the embedded indexer standalone. Rejected
  at startup for now: the indexer has no token-bridge data source yet.

The embedded indexer is the acropolis `midnight_indexer` module fleet
(Mithril snapshot + Ouroboros peer ingest -> in-memory Midnight indexes),
spawned on the node's tokio runtime and queried by calling its query service
directly - no gRPC transport, no codec, no local port (the indexer's TCP
gRPC server is disabled in the shipped configs). The five data-source
implementations are ported from Sundae Labs'
`whankinsiv/midnight-node-acropolis` parity-tested fork, updated to the
current data-source traits (`StableBlockByHashResult` classification,
Cardano health probes, `utxo_overestimate`). New `data-sources` crate;
acropolis is consumed as git dependencies pinned to a single rev of the
`gilescope/acropolis` fork (upstream 869354b3 plus two upstream-able
commits: a `tracing-subscriber` pin relax that otherwise conflicts with
polkadot-sdk, and the embeddable query-service surface). All of this is
behind the off-by-default `embedded-follower` cargo feature, so stock builds
are unaffected; without the feature the new backends fail at startup with a
clear error.

Config: `acropolis_config_file` points at an acropolis indexer TOML — presets
for mainnet, preview and guardnet ship in `res/cfg/acropolis/`, with the
indexer's own gRPC server disabled.

The comparison never blocks or delays the primary path: shadow queries run on
spawned tasks behind a bounded in-flight cap and are dropped (counted) when
the cap is hit.

PR:
