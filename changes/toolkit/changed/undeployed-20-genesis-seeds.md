#toolkit #genesis
# Undeployed genesis funds 20 well-known wallets

The `undeployed` (dev) genesis now funds 20 wallets, listed in `res/dev/undeployed-genesis-seeds.json`.
`wallet-seed-0` to `wallet-seed-3` are unchanged; `wallet-seed-N` for N >= 4 is `0x…20NN`.
20 keeps the genesis block within the ledger's block limits (`bytes_written` binds at 22).

`generate-genesis` now funds `wallet-seed-N` in numeric order, and toolkit_e2e runs on the dev
genesis instead of its own.

Supersedes #1521.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/1345
