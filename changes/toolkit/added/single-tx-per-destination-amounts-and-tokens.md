#toolkit #generate-txs
# Per-destination amounts and token types in `generate-txs single-tx`

`generate-txs single-tx` now supports two CLI shapes for specifying its
destinations; either can be used per invocation, but they cannot be mixed:

1. `--output addr=<bech32>,amount=<u128>[,token=<32-byte-hex>]` — one
   repeatable flag, one destination per occurrence, with the triple
   bundled in a single argument. The address HRP picks shielded vs
   unshielded; omitting `token` defaults it to the all-zeros token type.

2. The previous form, `--destination-address` plus parallel
   `--shielded-amount` / `--shielded-token-type` / `--unshielded-amount` /
   `--unshielded-token-type` flags. Each per-side flag may be provided
   once to broadcast or once per destination to align by command-line
   order. Backwards-compatible with prior usage.

A single transaction can now mix multiple shielded and/or unshielded token
types in its outputs. Coin/UTXO selection runs separately per token type,
with one change refund per token type back to the source/funding wallet.

Example — one mixed-token tx with one unshielded NIGHT output and one
shielded output of a different token type, to two different destinations,
using the bundled-triple shape:

```
midnight-node-toolkit generate-txs single-tx \
  --source-seed <SEED> \
  --output addr=mn_addr1...,amount=410000000,token=0000...0000 \
  --output addr=mn_shield-addr1...,amount=41,token=0000...0001
```

The same tx using the parallel-flag shape:

```
midnight-node-toolkit generate-txs single-tx \
  --source-seed <SEED> \
  --destination-address mn_addr1... \
  --unshielded-amount 410000000 \
  --unshielded-token-type 0000...0000 \
  --destination-address mn_shield-addr1... \
  --shielded-amount 41 \
  --shielded-token-type 0000...0001
```

Notes:
* `--input-utxo` is only supported when exactly one unshielded token type
  is used across the tx (the pinned UTXOs must all share that token type).
* In the parallel-flag shape, mismatched flag counts (e.g. 3 destinations
  on a side but 2 amounts) are rejected up front with a clear error.

PR: https://github.com/midnightntwrk/midnight-node/pull/1560
