#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Fresh non-validator node for S1 / S2: empty volume, syncs from node-1.
# SYNC_MODE=full replays history from genesis (S1, default pruning so
# finalized-block pruning runs while importing); SYNC_MODE=warp warp-syncs to
# the finalized head (S2).

set -euxo pipefail

. /shared/mc.env

./midnight-node \
  --chain=/shared/chain-spec.json \
  --node-key="$NODE_KEY" \
  --port="$P2P_PORT" \
  --bootnodes="/dns/midnight-node-1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" \
  --base-path=/data \
  --unsafe-rpc-external \
  --rpc-methods=Unsafe \
  --rpc-port="$RPC_PORT" \
  --rpc-cors=all \
  --prometheus-port="$PROMETHEUS_PORT" \
  --prometheus-external \
  --sync="$SYNC_MODE"
