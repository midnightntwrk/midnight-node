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

# Contract compiler image with Aiken for compiling governance contracts
# Uses Node.js base image since aiken is installed via npm in build_contracts.sh

# renovate: datasource=docker depName=node
FROM node:22-slim@sha256:7378f5a4830ef48eb36d1abf4ef398391db562b5c41a0bded83192fbcea21cc8

# Install required tools
# renovate: datasource=debian depName=jq versioning=deb
RUN apt-get update && \
    apt-get install -y --no-install-recommends jq=1.6-2.1+deb12u1 && \
    rm -rf /var/lib/apt/lists/*

# Install aiken via npm (same pattern as Earthfile rebuild-redemption-skeleton)
# renovate: datasource=npm packageName=aiken-lang/aiken
ENV AIKEN_VERSION=1.1.19
RUN npm install -g @aiken-lang/aiken@${AIKEN_VERSION}

# Create non-root user for security
RUN useradd -u 10001 -m -s /bin/bash appuser

WORKDIR /contracts

COPY compile-contracts.sh /compile-contracts.sh
RUN chmod +x /compile-contracts.sh && \
    chown -R appuser:appuser /contracts

USER appuser

# One-shot container: compilation completes and exits, healthcheck is a no-op
HEALTHCHECK --interval=60s --timeout=5s --retries=1 --start-period=5s CMD exit 0

ENTRYPOINT ["/bin/bash", "/compile-contracts.sh"]
