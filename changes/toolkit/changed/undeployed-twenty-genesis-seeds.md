#devnet #genesis #toolkit
# Undeployed network: twenty pre-funded genesis wallets

The `undeployed` ledger genesis funds twenty well-known wallets (`wallet-seed-0` … `wallet-seed-19`). Deterministic hex seeds live in `res/dev/undeployed-genesis-seeds.json`; `wallet-seed-3` is unchanged (Lace test). `generate-genesis` orders `wallet-seed-N` keys by numeric `N` for stable processing.

Regenerate artifacts with Earthly (faster locally: `--TOOLKIT_IMAGE=midnightntwrk/midnight-node-toolkit:latest-main` on `+rebuild-genesis-state` to skip compiling the toolkit). Commit updated `res/genesis/genesis_{state,block}_undeployed.mn`, `util/toolkit/test-data/genesis/*`, and related `SAVE ARTIFACT` outputs.

Earthly may log `Ledger block limit exceeded … clamping` during genesis; the build still completes and writes genesis files.

PR: (add after opening PR)
JIRA: (if applicable)
