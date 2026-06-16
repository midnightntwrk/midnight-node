#node
# Add stagenet network genesis and chain specs

Add the `stagenet` network. Stagenet runs on Cardano preview with 7 permissioned
validators, with governance split across a technical committee (validators 1-3)
and council (validators 4-6).

Includes:
- Config preset `res/cfg/stagenet.toml`
- Cardano preview bridge, governance, and candidate configs under `res/stagenet/`
- Generated genesis ledger state `res/genesis/genesis_{state,block}_stagenet.mn`
  and chain specs `res/stagenet/chain-spec{,-raw,-abridged}.json`
- Earthfile targets `generate-stagenet-genesis-seeds` and `rebuild-genesis-state-stagenet`

The cNIGHT observation state starts empty and is observed forward from genesis.
The reserve and ICS (treasury) pools are seeded with nominal genesis amounts
(reserve 6000000000873988, treasury 1200000000000000 NIGHT), matching main's
dev/devnet networks; faucet wallets are funded from the reserve. (Ledger 9
requires non-empty pools.)

PR: https://github.com/midnightntwrk/midnight-node/pull/1707
Issue: https://github.com/midnightntwrk/midnight-node/issues/1705
