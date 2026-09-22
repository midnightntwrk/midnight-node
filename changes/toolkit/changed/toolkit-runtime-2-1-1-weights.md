#toolkit
# Recognise the 2.1.1 runtime when fetching blocks

Bumping `spec_version` to `002_001_001` so the 2.1.0 benchmark weights (#2160) could be
enacted left the toolkit's block fetcher unable to read the chain that runtime produces:
`RuntimeVersion`'s `TryFrom<u32>` knows a closed set of spec versions, so every block after
the upgrade fails with `UnsupportedBlockVersion(2001001)`. The whole fetch path goes down
with it — `fetch`, `fund_wallets.py`, `register_dust.py`, `generate_txs_round_robin.py`,
`tx_load_applier.py` — while `send_batch_txs.py`, which takes neither `--fetch-cache` nor
`--ledger-state-db`, keeps working and makes it look like a load-generation fault.

`RuntimeVersion` gains a `V2_1_1` variant mapped to that spec version, reusing the 2.1.0
subxt metadata snapshot. Spec `2_001_001` differs from `2_001_000` only by dispatch weights,
which are not part of the metadata this decoder reads; the extrinsic envelope and event types
are unchanged and `transaction_version` stayed at 4. This is the same reasoning under which
`MidnightMetadata2_0_0` already reuses the 1.0.0 snapshot.

A `spec_version` bump and its `runtimes.rs` entry belong in the same commit. This one did not
get that, and the gap only showed up when a load run hit the upgraded chain.
