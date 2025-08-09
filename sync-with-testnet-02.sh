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

# Login to the correct cluster to access db-sync
# Even for testnet-02, we use the db-sync instance from the qanet cluster
# (both are running cardano preview)
tsh kube login k0-eks-tooling-stg-eu-01

export CFG_PRESET=testnet-02
export BOOTNODES="/dns/boot-node-01.testnet-02.midnight.network/tcp/30333/ws/p2p/12D3KooWMjUq13USCvQR9Y6yFzYNYgTQBLNAcmc8psAuPx2UUdnB /dns/boot-node-02.testnet-02.midnight.network/tcp/30333/ws/p2p/12D3KooWR1cHBUWPCqk3uqhwZqUFekfWj8T7ozK6S18DUT745v4d /dns/boot-node-03.testnet-02.midnight.network/tcp/30333/ws/p2p/12D3KooWQxxUgq7ndPfAaCFNbAxtcKYxrAzTxDfRGNktF75SxdX5"
./scripts/sync.sh
