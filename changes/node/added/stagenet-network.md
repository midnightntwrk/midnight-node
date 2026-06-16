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

The cNIGHT, reserve, and ICS observation state start empty and are observed
forward from genesis (the Cardano side starts unfunded). Reserve `total_amount`
of 0 assigns the remaining supply to the reserve pool, which funds the faucet
wallets.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1705
