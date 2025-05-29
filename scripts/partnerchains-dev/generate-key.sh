#!/bin/bash

export SEED_PHRASE=$(./partner-chains-node key generate --output-type json | grep Phrase | awk -F'"' '{print $4}')

echo "$SEED_PHRASE" > seed-phrase.txt

export GENESIS_UTXO="0000000000000000000000000000000000000000000000000000000000000000#0"
export COMMITTEE_CANDIDATE_ADDRESS="addr_10000"
export D_PARAMETER_POLICY_ID="00000000000000000000000000000000000000000000000000000000"
export PERMISSIONED_CANDIDATES_POLICY_ID="00000000000000000000000000000000000000000000000000000000"
export NATIVE_TOKEN_POLICY_ID="00000000000000000000000000000000000000000000000000000000"
export NATIVE_TOKEN_ASSET_NAME="00000000000000000000000000000000000000000000000000000000"
export ILLIQUID_SUPPLY_VALIDATOR_ADDRESS="00000000000000000000000000000000000000000000000000000000"

./partner-chains-node key generate-node-key --base-path ./data
./partner-chains-node key insert --base-path ./data --scheme ecdsa --key-type crch --suri "$SEED_PHRASE"
./partner-chains-node key insert --base-path ./data --scheme ed25519 --key-type gran --suri "$SEED_PHRASE"
./partner-chains-node key insert --base-path ./data --scheme sr25519 --key-type aura --suri "$SEED_PHRASE"

echo "Key generation complete. Seed phrase saved to: seed-phrase.txt"

./partner-chains-cli generate-keys

echo "Config file generated."
