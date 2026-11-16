#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# http://www.apache.org/licenses/LICENSE-2.0
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Regenerates the Cardano stake pool identity of midnight-node-3, -4 and -5.
#
# These nodes stand for Cardano SPOs that register as permissionless Midnight
# candidates, so each needs a pool of its own: a cold key to be named by, a VRF
# key because a genesis pool must declare one, and a stake key to hold the
# delegation that gives the pool its weight. None of them mints Cardano blocks
# — the one cardano-node in this repo does that, from a pool holding almost all
# of the stake — so no KES key or operational certificate is generated here.
#
# The stake key is paired with the node's existing payment key, so the address
# carrying the delegated funds is spendable with the payment.skey already in
# the node's directory.
#
# Running this invalidates the Shelley genesis in ../genesis/shelley: its
# staking section names these pools by the hashes of the keys below. Rerun
# update-genesis-pools.py afterwards.

set -euo pipefail

IMAGE="${CARDANO_IMAGE:-ghcr.io/intersectmbo/cardano-node:11.0.1}"
TESTNET_MAGIC="${TESTNET_MAGIC:-42}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cli() {
  local dir="$1"; shift
  docker run --rm --user "$(id -u):$(id -g)" \
    -v "$dir:/out" --entrypoint cardano-cli "$IMAGE" "$@"
}

for node in midnight-node-3 midnight-node-4 midnight-node-5; do
  dir="$HERE/$node/keys"
  echo "Generating Cardano pool keys for $node"

  cli "$dir" latest node key-gen \
    --cold-verification-key-file /out/cold.vkey \
    --cold-signing-key-file /out/cold.skey \
    --operational-certificate-issue-counter-file /out/cold.counter

  cli "$dir" latest node key-gen-VRF \
    --verification-key-file /out/vrf.vkey \
    --signing-key-file /out/vrf.skey

  cli "$dir" latest stake-address key-gen \
    --verification-key-file /out/stake.vkey \
    --signing-key-file /out/stake.skey

  # Only midnight-node-3 ships a payment verification key; the others have to
  # have theirs derived from the signing key they do ship.
  if [ ! -f "$dir/payment.vkey" ]; then
    cli "$dir" latest key verification-key \
      --signing-key-file /out/payment.skey \
      --verification-key-file /out/payment.vkey
  fi

  cli "$dir" latest stake-address build \
    --stake-verification-key-file /out/stake.vkey \
    --testnet-magic "$TESTNET_MAGIC" --out-file /out/stake.addr

  # Where this pool's delegated stake sits: the node's payment credential with
  # the stake credential above.
  cli "$dir" latest address build \
    --payment-verification-key-file /out/payment.vkey \
    --stake-verification-key-file /out/stake.vkey \
    --testnet-magic "$TESTNET_MAGIC" --out-file /out/payment.stake.addr

  cli "$dir" latest stake-pool id \
    --cold-verification-key-file /out/cold.vkey \
    --output-format hex --out-file /out/pool.id

  echo "  pool id: $(cat "$dir/pool.id")"
done
