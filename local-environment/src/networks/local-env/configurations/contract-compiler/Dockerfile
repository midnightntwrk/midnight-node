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

# Install required tools (curl needed for tarball download)
# Package versions for Debian Bookworm (node:22-slim base)
# renovate: datasource=debian depName=jq versioning=deb
# renovate: datasource=debian depName=curl versioning=deb
# renovate: datasource=debian depName=ca-certificates versioning=deb
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        jq=1.6-2.1+deb12u1 \
        curl=7.88.1-10+deb12u14 \
        ca-certificates=20230311+deb12u1 && \
    rm -rf /var/lib/apt/lists/*

# Declare TARGETARCH for multi-platform builds (auto-provided by BuildKit)
ARG TARGETARCH

# Install aiken: pre-built binary for amd64, build from source for arm64
# renovate: datasource=github-releases depName=aiken-lang/aiken
ENV AIKEN_VERSION=1.1.21
# To update hash: curl -sL "https://github.com/aiken-lang/aiken/releases/download/v${AIKEN_VERSION}/aiken-x86_64-unknown-linux-musl.tar.gz" | sha256sum
ENV AIKEN_SHA256_AMD64=9fe0bb94efbb79f4d351664def3da0d58c7e30716fcb65590ff9b6c6b0ffb715
RUN if [ "${TARGETARCH}" = "amd64" ]; then \
        curl -fsSL "https://github.com/aiken-lang/aiken/releases/download/v${AIKEN_VERSION}/aiken-x86_64-unknown-linux-musl.tar.gz" -o /tmp/aiken.tgz && \
        echo "${AIKEN_SHA256_AMD64}  /tmp/aiken.tgz" | sha256sum -c - && \
        tar -xzf /tmp/aiken.tgz -C /tmp && \
        mv /tmp/aiken-x86_64-unknown-linux-musl/aiken /usr/local/bin/aiken && \
        rm -rf /tmp/aiken.tgz /tmp/aiken-x86_64-unknown-linux-musl; \
    else \
        apt-get update && \
        apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev && \
        rm -rf /var/lib/apt/lists/* && \
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal && \
        . "$HOME/.cargo/env" && \
        cargo install aiken@${AIKEN_VERSION} --locked && \
        mv "$HOME/.cargo/bin/aiken" /usr/local/bin/aiken && \
        rustup self uninstall -y && \
        apt-get purge -y build-essential pkg-config libssl-dev && \
        apt-get autoremove -y; \
    fi && \
    chmod +x /usr/local/bin/aiken

# Install just command runner from GitHub releases
# renovate: datasource=github-releases depName=casey/just
ENV JUST_VERSION=1.46.0
# To update hashes: curl -sL "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-<arch>-unknown-linux-musl.tar.gz" | sha256sum
ENV JUST_SHA256_AMD64=79966e6e353f535ee7d1c6221641bcc8e3381c55b0d0a6dc6e54b34f9db36eaa
ENV JUST_SHA256_ARM64=b81970c8247fa64cfb30d2a3da0e487e4253f9f2d01865ed5e7d53cdc7b02188
RUN ARCH=$(case ${TARGETARCH} in amd64) echo "x86_64";; arm64) echo "aarch64";; *) echo "x86_64";; esac) && \
    JUST_SHA256=$(case ${TARGETARCH} in amd64) echo "${JUST_SHA256_AMD64}";; arm64) echo "${JUST_SHA256_ARM64}";; *) echo "${JUST_SHA256_AMD64}";; esac) && \
    curl -fsSL "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-${ARCH}-unknown-linux-musl.tar.gz" -o /tmp/just.tar.gz && \
    echo "${JUST_SHA256}  /tmp/just.tar.gz" | sha256sum -c - && \
    tar -xzf /tmp/just.tar.gz -C /usr/local/bin just && \
    chmod +x /usr/local/bin/just && \
    rm /tmp/just.tar.gz

# Install toml-cli: pre-built binary for amd64, build from source for arm64
# renovate: datasource=github-releases depName=gnprice/toml-cli
ENV TOML_CLI_VERSION=0.2.3
# To update hash: curl -sL "https://github.com/gnprice/toml-cli/releases/download/v${TOML_CLI_VERSION}/toml-${TOML_CLI_VERSION}-x86_64-linux.tar.gz" | sha256sum
ENV TOML_CLI_SHA256_AMD64=ba12ae6b53fc593a9dcae3d6ef5d50f0382b3f77708603dc237e9145ba7988fc
RUN if [ "${TARGETARCH}" = "amd64" ]; then \
        curl -fsSL "https://github.com/gnprice/toml-cli/releases/download/v${TOML_CLI_VERSION}/toml-${TOML_CLI_VERSION}-x86_64-linux.tar.gz" -o /tmp/toml.tar.gz && \
        echo "${TOML_CLI_SHA256_AMD64}  /tmp/toml.tar.gz" | sha256sum -c - && \
        tar -xzf /tmp/toml.tar.gz -C /tmp && \
        mv /tmp/toml-${TOML_CLI_VERSION}-x86_64-linux/toml /usr/local/bin/toml && \
        rm -rf /tmp/toml.tar.gz /tmp/toml-${TOML_CLI_VERSION}-x86_64-linux; \
    else \
        apt-get update && \
        apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev && \
        rm -rf /var/lib/apt/lists/* && \
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal && \
        . "$HOME/.cargo/env" && \
        cargo install toml-cli@${TOML_CLI_VERSION} --locked && \
        mv "$HOME/.cargo/bin/toml" /usr/local/bin/toml && \
        rustup self uninstall -y && \
        apt-get purge -y build-essential pkg-config libssl-dev && \
        apt-get autoremove -y; \
    fi && \
    chmod +x /usr/local/bin/toml

# Install bun JavaScript runtime from GitHub releases
# renovate: datasource=github-releases depName=oven-sh/bun
ENV BUN_VERSION=1.3.6
# To update hash: curl -sL "https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-linux-<arch>.zip" | sha256sum
ENV BUN_SHA256_AMD64=9ba98d2134550d6690875b23a4f5c48e74b7cb267e8cc1b8f52605921c6c11ef
ENV BUN_SHA256_ARM64=5afd12b366ba2d8297245cc29c039416334dd872152c1db02e5c8aa8c66e96b1
RUN apt-get update && \
    apt-get install -y --no-install-recommends unzip=6.0-28 && \
    rm -rf /var/lib/apt/lists/* && \
    ARCH=$(case ${TARGETARCH} in amd64) echo "x64";; arm64) echo "aarch64";; *) echo "x64";; esac) && \
    BUN_SHA256=$(case ${TARGETARCH} in amd64) echo "${BUN_SHA256_AMD64}";; arm64) echo "${BUN_SHA256_ARM64}";; *) echo "${BUN_SHA256_AMD64}";; esac) && \
    curl -fsSL "https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-linux-${ARCH}.zip" -o /tmp/bun.zip && \
    echo "${BUN_SHA256}  /tmp/bun.zip" | sha256sum -c - && \
    unzip -q /tmp/bun.zip -d /tmp && \
    mv /tmp/bun-linux-${ARCH}/bun /usr/local/bin/bun && \
    chmod +x /usr/local/bin/bun && \
    ln -s /usr/local/bin/bun /usr/local/bin/bunx && \
    rm -rf /tmp/bun.zip /tmp/bun-linux-${ARCH}

# Create non-root user for security
RUN useradd -u 10001 -m -s /bin/bash appuser

WORKDIR /contracts

COPY compile-contracts.sh /compile-contracts.sh
COPY .env /.env
RUN chmod +x /compile-contracts.sh && \
    chown -R appuser:appuser /contracts

USER appuser

# One-shot container: compilation completes and exits, healthcheck is a no-op
HEALTHCHECK --interval=60s --timeout=5s --retries=1 --start-period=5s CMD exit 0

ENTRYPOINT ["/bin/bash", "/compile-contracts.sh"]
