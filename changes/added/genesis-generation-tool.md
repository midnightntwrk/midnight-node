#node
# Added interactive genesis generation tool

Added `scripts/genesis/genesis-generation.sh`, an interactive shell script that guides users through the chain specification generation process. The tool walks through three main steps:

1. **Ledger State Generation** - Creates initial ledger state files (genesis_block, genesis_state) using Earthly
2. **Genesis Config Generation** - Generates config files from smart contract addresses on Cardano (cnight-config.json, federated-authority-config.json, permissioned-candidates-config.json)
3. **Chain Spec Generation** - Creates the final chain specification files using Earthly

The tool collects common inputs upfront (DB connection string, Cardano tip, RNG seed) and allows users to selectively run each step. For networks that use cNight genesis (like qanet), it handles the dependency where cnight-config.json must be generated before ledger state generation.

Also added `--pc-config` CLI argument to `generate-permissioned-candidates-genesis` and `generate-genesis-config` commands to allow reading `security_parameter` from `pc-chain-config.json` when `CARDANO_SECURITY_PARAMETER` env var is not set.

PR :https://github.com/midnightntwrk/midnight-node/pull/582
JIRA: https://shielded.atlassian.net/browse/PM-20830
