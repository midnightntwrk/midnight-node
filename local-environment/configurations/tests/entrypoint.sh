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

# Verify the e2e-tests directory exists and cd into it
if [ -d "/e2e-tests" ]; then
  cd /e2e-tests
else
  echo "Error: Directory /e2e-tests does not exist. Ensure e2e-tests directory was copied to ./configuration/tests/e2e-tests/ before bringing up the container"
  exit 1
fi

# Install Docker CLI for running Docker commands in other containers
apt-get update && apt-get install -y docker.io

# Install pytest dependencies
apt-get update && \
apt-get install -y curl && \
curl -L --silent https://github.com/getsops/sops/releases/download/v3.7.3/sops_3.7.3_amd64.deb > sops.deb && \
dpkg -i sops.deb && \
rm sops.deb && \
apt-get clean && \
rm -rf /var/lib/apt/lists/*

# Create and initialize the virtual environment
python -m venv venv
source venv/bin/activate
pip install --upgrade pip
pip install -r requirements.txt

# Keep the container running
tail -f /dev/null
