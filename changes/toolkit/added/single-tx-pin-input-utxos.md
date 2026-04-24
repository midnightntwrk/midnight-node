#toolkit #generate-txs
# Pin specific UTXOs as inputs to generate-txs single-tx

Adds a repeatable `--input-utxo <intent_hash_hex>#<output_no>` flag to
`generate-txs single-tx`. When provided, the toolkit skips its built-in
greedy coin selection for the unshielded transfer and uses exactly the
listed UTXOs as inputs. The summed value must cover
`--unshielded-amount * <destinations>`; any excess becomes change back
to the source wallet. Missing UTXOs (wrong owner, wrong token, or not
in the wallet's current state) produce an error instead of silently
falling back to the default selector.

Useful for deterministic UTXO consolidation when the default
"first-fit-that-overshoots, else tail" selector picks an unwanted
subset of a skewed wallet.

PR: https://github.com/midnightntwrk/midnight-node/pull/1404
