#!/bin/bash

WALLET_DIR="$PWD/governance-wallet"

mkdir -p $WALLET_DIR

./cardano-cli address key-gen \
--verification-key-file $WALLET_DIR/payment.vkey \
--signing-key-file $WALLET_DIR/payment.skey

./cardano-cli address build \
--payment-verification-key-file $WALLET_DIR/payment.vkey \
--out-file $WALLET_DIR/payment.addr \

echo "Wallet created. Files saved to: $WALLET_DIR"
echo "Address: $(cat $WALLET_DIR/payment.addr)"
