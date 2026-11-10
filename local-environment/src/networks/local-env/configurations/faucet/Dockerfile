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

# Faucet image with a newer wallet-sdk-shielded build swapped in. Until the faucet ships
# a release with midnightntwrk/midnight-wallet#924 fixed, its shielded wallet never
# reports synced on a fresh local-env chain, so it never drips.
ARG FAUCET_IMAGE
FROM ${FAUCET_IMAGE}
ARG SHIELDED_SDK_VERSION
# Only dist is replaced, so the faucet keeps its own copies of the sibling SDK packages.
RUN test -n "${SHIELDED_SDK_VERSION}" \
 && cd /tmp \
 && npm pack --silent "@midnightntwrk/wallet-sdk-shielded@${SHIELDED_SDK_VERSION}" \
 && tar -xzf midnightntwrk-wallet-sdk-shielded-*.tgz \
 && rm -rf /source/node_modules/@midnightntwrk/wallet-sdk-shielded/dist \
 && mv package/dist /source/node_modules/@midnightntwrk/wallet-sdk-shielded/dist \
 && rm -rf /tmp/package /tmp/midnightntwrk-wallet-sdk-shielded-*.tgz
