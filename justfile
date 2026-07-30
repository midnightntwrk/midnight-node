# Justfile for Midnight Node
# This Justfile is used to define tasks for building, testing, and running the Midnight Node.

# Build or fetch compactc from the `compact/` submodule and expose it to toolkit-js via
# COMPACT_HOME (run once, and after bumping the submodule).
compactc compact_repo="LFDT-Minokawa/compact" compact_tag_prefix="compactc-v":
  COMPACTC_SUBMODULE_VERSION=$(bash scripts/compact-submodule-version.sh); \
  COMPACTC_VERSION=$(cat COMPACTC_VERSION); \
  if [ "$COMPACTC_VERSION" = "$COMPACTC_SUBMODULE_VERSION" ]; then \
      earthly +compactc-build-local; \
    else \
      earthly +compactc-fetch-local \
        --VERSION="$COMPACTC_VERSION" \
        --COMPACT_REPO={{compact_repo}} \
        --COMPACT_TAG_PREFIX={{compact_tag_prefix}}; \
    fi

toolkit-update-ledger-parameters-e2e NODE_IMAGE TOOLKIT_IMAGE:
  @scripts/tests/toolkit-update-ledger-parameters-e2e.sh {{NODE_IMAGE}} {{TOOLKIT_IMAGE}}
  @echo "✅ Toolkit Update Ledger Parameters E2E test completed successfully."

toolkit-e2e NODE_IMAGE TOOLKIT_IMAGE:
  @scripts/tests/toolkit-e2e.sh {{NODE_IMAGE}} {{TOOLKIT_IMAGE}}
  @echo "✅ Toolkit E2E test completed successfully."

toolkit-maintenance-e2e NODE_IMAGE TOOLKIT_IMAGE:
  @scripts/tests/toolkit-maintenance-e2e.sh {{NODE_IMAGE}} {{TOOLKIT_IMAGE}}
  @echo "✅ Toolkit Maintenance E2E test completed successfully."

toolkit-contracts-e2e NODE_IMAGE TOOLKIT_IMAGE:
  @scripts/tests/toolkit-contracts-e2e.sh {{NODE_IMAGE}} {{TOOLKIT_IMAGE}}
  @echo "✅ Toolkit Contracts E2E test completed successfully."

toolkit-mint-e2e NODE_IMAGE="" TOOLKIT_IMAGE="":
  @scripts/tests/toolkit-mint-e2e.sh {{NODE_IMAGE}} {{TOOLKIT_IMAGE}}
  @echo "✅ Toolkit Mint E2E test completed successfully."

toolkit-tokens-minter-e2e NODE_IMAGE="" TOOLKIT_IMAGE="":
  @scripts/tests/toolkit-tokens-minter-e2e.sh {{NODE_IMAGE}} {{TOOLKIT_IMAGE}}
  @echo "✅ Toolkit Tokens Minter E2E test completed successfully."

startup-dev-e2e NODE_IMAGE:
  @scripts/tests/startup-dev-e2e.sh {{NODE_IMAGE}}
  @echo "✅ Startup E2E test in dev mode completed successfully."

startup-qanet-e2e NODE_IMAGE:
  @scripts/tests/startup-qanet-e2e.sh {{NODE_IMAGE}}
  @echo "✅ Startup E2E test in qanet mode completed successfully."

genesis-wallets-undeployed-e2e NODE_IMAGE TOOLKIT_IMAGE:
  @scripts/tests/genesis-wallets-undeployed-e2e.sh {{NODE_IMAGE}} {{TOOLKIT_IMAGE}}
  @echo "✅ Genesis wallet E2E test in undeployed network completed successfully."

genesis-wallets-devnet-e2e NODE_IMAGE TOOLKIT_IMAGE:
  @scripts/tests/genesis-wallets-devnet-e2e.sh {{NODE_IMAGE}} {{TOOLKIT_IMAGE}}
  @echo "✅ Genesis wallet E2E test in devnet network completed successfully."

indexer-api-e2e:
  @scripts/tests/indexer-api-e2e.sh
  @echo "✅ Indexer GraphQL API E2E test completed successfully."
