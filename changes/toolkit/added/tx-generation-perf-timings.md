#toolkit #perf
# Add per-phase timing logs for transaction generation

`single-tx` and `batch-single-tx` now emit `[perf]` debug-level log lines for each phase
of building a transaction: coin/UTXO selection (`select_shielded_offer` /
`select_unshielded_intents`), offer/intent construction (`build_offer_intents`), the fee
balancing loop (`pay_fees`, including the iteration count), real PLONK proving
(`prove_tx`), and serialization (`serialize`). Enable with `--verbose` (or
`RUST_LOG=debug`) to see the breakdown; a report script can grep for `[perf]` lines and
aggregate per-phase trend lines across a run.

PR: https://github.com/midnightntwrk/midnight-node/pull/TODO
Issue: https://github.com/shieldedtech/midnight-performance/issues/292
