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

#!/bin/bash

echo "Waiting for Cardano chain to start..."

while true; do
    if [ -f "/shared/cardano.ready" ]; then
        break
    else
        sleep 1
    fi
done

echo "Cardano chain ready. Starting Ogmios..."

exec /bin/ogmios \
  --host=0.0.0.0 \
  --node-config=/shared/node-1-config.json \
  --node-socket=/node-ipc/node.socket &

wait
