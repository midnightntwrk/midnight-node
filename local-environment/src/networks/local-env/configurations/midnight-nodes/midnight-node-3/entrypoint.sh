#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) 2025 Midnight Foundation
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

# Fail if a command fails
set -euxo pipefail

# Source the mainchain env
. /shared/mc.env

./midnight-node \
  --charlie \
  --chain=/shared/chain-spec.json \
  --node-key=0000000000000000000000000000000000000000000000000000000000000003 \
  --bootnodes="/dns/midnight-node-1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" \
  --base-path=/data \
  --unsafe-rpc-external \
  --rpc-methods=Unsafe \
  --rpc-port=9935 \
  --rpc-cors=all \
  --prometheus-port=9617 \
  --prometheus-external \
  --state-pruning=archive \
  --blocks-pruning=archive \
  --enable-offchain-indexing true &

  touch /shared/midnight-node-3.ready

  wait
