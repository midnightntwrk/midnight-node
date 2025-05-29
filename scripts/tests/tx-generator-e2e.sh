#!/usr/bin/env bash
set -euxo pipefail

NODE_IMAGE="$1"
GENERATOR_IMAGE="$2"
RNG_SEED="0000000000000000000000000000000000000000000000000000000000000037"

echo "🎯 Running TX Generator E2E test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 GENERATOR_IMAGE: $GENERATOR_IMAGE"

# Ensure Docker network exists
docker network create midnight-net-tx || true

# Start node in background
echo "🚀 Starting node container..."
docker run -d --rm \
  --name midnight-node-tx \
  --network midnight-net-tx \
  -p 9944:9944 \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "$NODE_IMAGE"

echo "⏳ Waiting for node to boot..."
sleep 10

# Run generator commands
echo "📦 Running transaction generator tests..."

tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t 'txgene2e')
deploy_filename="contract_deploy.mn"
address_filename="contract_address.mn"

docker run --rm -e RUST_BACKTRACE=1 --network host "$GENERATOR_IMAGE" generate-txs batches -n 1 -b 1

docker run --rm -e RUST_BACKTRACE=1 -v $tempdir:/out --network host "$GENERATOR_IMAGE" generate-txs \
    --dest-file "/out/$deploy_filename" --to-bytes \
    contract-calls deploy --rng-seed "$RNG_SEED"
docker run --rm -e RUST_BACKTRACE=1 -v $tempdir:/out --network host "$GENERATOR_IMAGE" contract-address --network undeployed --src-file "/out/$deploy_filename" --dest-file "/out/$address_filename"
docker run --rm -e RUST_BACKTRACE=1 -v $tempdir:/out --network host "$GENERATOR_IMAGE" generate-txs \
    --src-files="/out/$deploy_filename" send

docker run --rm -e RUST_BACKTRACE=1 -v $tempdir:/out --network host "$GENERATOR_IMAGE" generate-txs contract-calls maintenance --rng-seed "$RNG_SEED" --contract-address "/out/$address_filename"
docker run --rm -e RUST_BACKTRACE=1 -v $tempdir:/out --network host "$GENERATOR_IMAGE" generate-txs contract-calls call --call-key store --rng-seed "$RNG_SEED" --contract-address "/out/$address_filename"
docker run --rm -e RUST_BACKTRACE=1 -v $tempdir:/out --network host "$GENERATOR_IMAGE" generate-txs contract-calls call --call-key check --rng-seed "$RNG_SEED" --contract-address "/out/$address_filename"

rm -rf $tempdir

echo "🛑 Killing node container..."
docker kill midnight-node-tx

echo "✅ TX Generator E2"
