#node #genesis
# Add srtool for deterministic runtime WASM builds

Added support for deterministic runtime WASM builds using [srtool](https://github.com/paritytech/srtool):

- New `+srtool-build` target: Builds the runtime WASM deterministically using the srtool Docker image
- New `+srtool-info` target: Displays information about srtool configuration
- New `DETERMINISTIC` flag for `+rebuild-chainspec`: When `true`, uses srtool-built WASM
- New `+rebuild-chainspec-deterministic` convenience target

Usage:
```bash
# Build deterministic runtime WASM only
earthly +srtool-build

# Build chainspec with deterministic WASM
earthly +rebuild-chainspec --NETWORK=mainnet --DETERMINISTIC=true

# Or use the convenience target
earthly +rebuild-chainspec-deterministic --NETWORK=mainnet
```

The srtool digest (containing WASM hash and build info) is saved alongside the chain-spec for verification.

The genesis generation script (`scripts/genesis/genesis-generation.sh`) now prompts the user during Step 3 (Chain Spec Generation) whether to use a deterministic srtool build. When selected, it passes `--DETERMINISTIC=true` to the `+rebuild-chainspec` Earthly target. After generation, the script also creates a `chain-spec-hash.json` file containing the SHA-256 hash of `chain-spec-raw.json`.

PR: https://github.com/midnightntwrk/midnight-node/pull/681
JIRA: https://shielded.atlassian.net/browse/PM-21907
