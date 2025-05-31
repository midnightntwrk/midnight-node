#!/bin/bash

if [ -z "$1" ]; then
  echo "Usage: $0 <network-name>"
  exit 1
fi
NETWORK=$1
CHAIN_CONFIG="res/$NETWORK/partner-chains-cli-chain-config.json"
GOVERNANCE_SECRET="res/$NETWORK/governance.skey"
GENESIS_UTXO=$(jq -r '.chain_parameters.genesis_utxo' < "$GENESIS_UTXO")

set +x

(
./pc-contracts-cli addresses \
    --genesis-utxo $GENESIS_UTXO \
	--payment-signing-key-file "$GOVERNANCE_SECRET" \
	--atms-kind plain-ecdsa-secp256k1 \
	--ogmios-host ogmios.preview.midnight.network \
	--ogmios-port 443 \
	--ogmios-secure \
	--kupo-host kupo.preview.midnight.network \
	--kupo-port 443 \
	--kupo-secure \
	--network testnet | jq > "res/$NETWORK/addresses.json"
)

echo "Wrote addresses to res/$NETWORK/addresses.json"
