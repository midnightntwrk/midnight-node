#!/bin/bash

 # First argument is network name - if it doesn't exist, print usage
if [ -z "$1" ]; then
  echo "Usage: $0 <network-name>"
  exit 1
fi
NETWORK=$1
CHAIN_CONFIG="res/$NETWORK/pc-chain-config.json"
GOVERNANCE_SECRET="res/$NETWORK/governance.skey"
GOVERNANCE_VERIFICATION="res/$NETWORK/governance.vkey"
 
# Check the network name exists as a file
if [ ! -f "$CHAIN_CONFIG" ]; then
  echo "Chain config file $CHAIN_CONFIG does not exist."
  exit 1
fi

# Check the governance secret exists as a file
if [ ! -f "$GOVERNANCE_SECRET" ]; then
  echo "Governance secret $GOVERNANCE_SECRET does not exist."
  exit 1
fi

cp "$CHAIN_CONFIG" pc-chain-config.json

(
  cat pc-chain-config.json |
  jq '. + {
    "cardano_payment_signing_key_file": "'$GOVERNANCE_SECRET'",
    "cardano_payment_verification_key_file": "'$GOVERNANCE_VERIFICATION'"
  }' > tmp.json
)
mv -f tmp.json pc-chain-config.json

echo "Generating key..."
./generate-key.sh

echo "Preparing configuration..."
./partner-chains-cli prepare-configuration

echo "Creating chain spec..."
./partner-chains-cli create-chain-spec
