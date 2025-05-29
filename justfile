# Justfile for Midnight Node
# This Justfile is used to define tasks for building, testing, and running the Midnight Node.

hardfork-e2e NODE_IMAGE UPGRADER_IMAGE:
  @scripts/tests/hardfork-e2e.sh {{NODE_IMAGE}} {{UPGRADER_IMAGE}}
  @echo "✅ Hardfork E2E test completed successfully."

ledger-rollback-e2e NODE_IMAGE UPGRADER_IMAGE:
  @scripts/tests/ledger-rollback-e2e.sh {{NODE_IMAGE}} {{UPGRADER_IMAGE}}
  @echo "✅ Ledger rollback E2E test completed successfully."

node-e2e NODE_IMAGE:
  @scripts/tests/node-e2e.sh {{NODE_IMAGE}}
  @echo "✅ Node E2E test completed successfully."

tx-generator-e2e NODE_IMAGE GENERATOR_IMAGE:
  @scripts/tests/tx-generator-e2e.sh {{NODE_IMAGE}} {{GENERATOR_IMAGE}}
  @echo "✅ TX Generator E2E test completed successfully."

startup-dev-e2e NODE_IMAGE:
  @scripts/tests/startup-dev-e2e.sh {{NODE_IMAGE}}
  @echo "✅ Startup E2E test in dev mode completed successfully."

startup-qanet-e2e NODE_IMAGE:
  @scripts/tests/startup-qanet-e2e.sh {{NODE_IMAGE}}
  @echo "✅ Startup E2E test in qanet mode completed successfully."

genesis-wallets-undeployed-e2e NODE_IMAGE GENERATOR_IMAGE:
  @scripts/tests/genesis-wallets-undeployed-e2e.sh {{NODE_IMAGE}} {{GENERATOR_IMAGE}}
  @echo "✅ Genesis wallet E2E test in undeployed network completed successfully."

genesis-wallets-devnet-e2e NODE_IMAGE GENERATOR_IMAGE:
  @scripts/tests/genesis-wallets-devnet-e2e.sh {{NODE_IMAGE}} {{GENERATOR_IMAGE}}
  @echo "✅ Genesis wallet E2E test in devnet network completed successfully."
