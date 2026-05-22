#toolkit #generate-txs
# Per-destination amounts and token types in `generate-txs single-tx`

`generate-txs single-tx` now accepts repeated `--shielded-amount`,
`--shielded-token-type`, `--unshielded-amount`, and `--unshielded-token-type`
flags, with one value per destination on each side (aligned by command-line
order). A single value still broadcasts to every destination on that side,
preserving the previous behaviour.

A single transaction can now mix multiple shielded and/or unshielded token
types in its outputs. Coin/UTXO selection runs separately per token type,
with one change refund per token type back to the source/funding wallet.

Example — one mixed-token tx with one unshielded NIGHT output and one
shielded output of a different token type, to two different destinations:

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
* Mismatched flag counts (e.g. 3 destinations on a side but 2 amounts) are
  rejected up front with a clear error.

PR:
