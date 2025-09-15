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

set -e

# Set the fake time
export FAKETIME="@$(date -d "@$EPOCH_TIME" '+%Y-%m-%d %H:%M:%S')"
export FAKETIME_DONT_FAKE_MONOTONIC=1
export LD_PRELOAD=$(find / -name 'libfaketime.so*' 2>/dev/null | head -n 1)

strings $(find /usr -name 'libfaketime.so*' | head -n 1) | grep FORCE_

echo "✅ LD_PRELOAD set to: $LD_PRELOAD"

RUST_LOG=midnight::ledger_v2 SIDECHAIN_BLOCK_BENEFICIARY=dca6896e7fe2f00a3d63be2168df8862cae24a770471e08c646d260db1625f CFG_PRESET=dev ./midnight-node
