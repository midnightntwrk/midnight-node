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

# Contract compiler image with Aiken for compiling governance contracts
# Multi-stage build: separate stages for AMD64 (binary downloads) and ARM64 (source builds)

# =============================================================================
# BASE STAGE: Common dependencies for all architectures
# =============================================================================
# renovate: datasource=docker depName=node
FROM node:22-slim@sha256:7378f5a4830ef48eb36d1abf4ef398391db562b5c41a0bded83192fbcea21cc8 AS base

# Install required tools
# Package versions for Debian Bookworm (node:22-slim base)
# renovate: datasource=debian depName=jq versioning=deb
# renovate: datasource=debian depName=curl versioning=deb
# renovate: datasource=debian depName=ca-certificates versioning=deb
# renovate: datasource=debian depName=vim versioning=deb
# renovate: datasource=debian depName=unzip versioning=deb
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        jq=1.6-2.1+deb12u1 \
        curl=7.88.1-10+deb12u14 \
        ca-certificates=20230311+deb12u1 \
        vim=2:9.0.1378-2+deb12u2 \
        unzip=6.0-28 && \
    rm -rf /var/lib/apt/lists/*

# Tool versions (shared across stages)
# renovate: datasource=github-releases depName=aiken-lang/aiken
ENV AIKEN_VERSION=1.1.21
# renovate: datasource=github-releases depName=casey/just
ENV JUST_VERSION=1.46.0
# renovate: datasource=github-releases depName=gnprice/toml-cli
ENV TOML_CLI_VERSION=0.2.3
# renovate: datasource=github-releases depName=oven-sh/bun
ENV BUN_VERSION=1.3.6

# =============================================================================
# AMD64 STAGE: Download pre-built binaries
# =============================================================================
FROM base AS tools-amd64

# SHA256 hashes for AMD64 binaries
# To update: curl -sL "<url>" | sha256sum
ENV AIKEN_SHA256=9fe0bb94efbb79f4d351664def3da0d58c7e30716fcb65590ff9b6c6b0ffb715
ENV JUST_SHA256=79966e6e353f535ee7d1c6221641bcc8e3381c55b0d0a6dc6e54b34f9db36eaa
ENV TOML_CLI_SHA256=ba12ae6b53fc593a9dcae3d6ef5d50f0382b3f77708603dc237e9145ba7988fc
ENV BUN_SHA256=1954abee08900de610a8b4692f5fffa6c6b15d8afde7d1de4d3f88cfc60e7d5f

# Install aiken
RUN curl -fsSL "https://github.com/aiken-lang/aiken/releases/download/v${AIKEN_VERSION}/aiken-x86_64-unknown-linux-musl.tar.gz" -o /tmp/aiken.tgz && \
    echo "${AIKEN_SHA256}  /tmp/aiken.tgz" | sha256sum -c - && \
    tar -xzf /tmp/aiken.tgz -C /tmp && \
    mv /tmp/aiken-x86_64-unknown-linux-musl/aiken /usr/local/bin/aiken && \
    chmod +x /usr/local/bin/aiken && \
    rm -rf /tmp/aiken.tgz /tmp/aiken-x86_64-unknown-linux-musl

# Install just
RUN curl -fsSL "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-x86_64-unknown-linux-musl.tar.gz" -o /tmp/just.tar.gz && \
    echo "${JUST_SHA256}  /tmp/just.tar.gz" | sha256sum -c - && \
    tar -xzf /tmp/just.tar.gz -C /usr/local/bin just && \
    chmod +x /usr/local/bin/just && \
    rm /tmp/just.tar.gz

# Install toml-cli
RUN curl -fsSL "https://github.com/gnprice/toml-cli/releases/download/v${TOML_CLI_VERSION}/toml-${TOML_CLI_VERSION}-x86_64-linux.tar.gz" -o /tmp/toml.tar.gz && \
    echo "${TOML_CLI_SHA256}  /tmp/toml.tar.gz" | sha256sum -c - && \
    tar -xzf /tmp/toml.tar.gz -C /tmp && \
    mv /tmp/toml-${TOML_CLI_VERSION}-x86_64-linux/toml /usr/local/bin/toml && \
    chmod +x /usr/local/bin/toml && \
    rm -rf /tmp/toml.tar.gz /tmp/toml-${TOML_CLI_VERSION}-x86_64-linux

# Install bun
RUN curl -fsSL "https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-linux-x64-baseline.zip" -o /tmp/bun.zip && \
    echo "${BUN_SHA256}  /tmp/bun.zip" | sha256sum -c - && \
    unzip -q /tmp/bun.zip -d /tmp && \
    mv /tmp/bun-linux-x64-baseline/bun /usr/local/bin/bun && \
    chmod +x /usr/local/bin/bun && \
    ln -s /usr/local/bin/bun /usr/local/bin/bunx && \
    rm -rf /tmp/bun.zip /tmp/bun-linux-x64-baseline

# =============================================================================
# ARM64 STAGE: Build from source where pre-built binaries unavailable
# =============================================================================
FROM base AS tools-arm64

# SHA256 hashes for ARM64 binaries (just and bun have ARM64 releases)
# To update: curl -sL "<url>" | sha256sum
ENV JUST_SHA256=b81970c8247fa64cfb30d2a3da0e487e4253f9f2d01865ed5e7d53cdc7b02188
ENV BUN_SHA256=5afd12b366ba2d8297245cc29c039416334dd872152c1db02e5c8aa8c66e96b1

# Install Rust build dependencies (version-pinned for Debian Bookworm)
# renovate: datasource=debian depName=build-essential versioning=deb
# renovate: datasource=debian depName=pkg-config versioning=deb
# renovate: datasource=debian depName=libssl-dev versioning=deb
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        build-essential=12.9 \
        pkg-config=1.8.1-1 \
        libssl-dev=3.0.18-1~deb12u2 && \
    rm -rf /var/lib/apt/lists/*

# Install Rust toolchain via pinned rustup-init binary with hash verification
# renovate: datasource=github-releases depName=rust-lang/rustup
ENV RUSTUP_VERSION=1.28.1
ENV RUSTUP_SHA256=c64b33db2c6b9385817ec0e49a84bcfe018ed6e328fe755c3c809580cc70ce7a
RUN curl -fsSL "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/aarch64-unknown-linux-gnu/rustup-init" -o /tmp/rustup-init && \
    echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - && \
    chmod +x /tmp/rustup-init && \
    /tmp/rustup-init -y --profile minimal && \
    rm /tmp/rustup-init
ENV PATH="/root/.cargo/bin:${PATH}"

# Build aiken from source
RUN cargo install aiken@${AIKEN_VERSION} --locked && \
    mv /root/.cargo/bin/aiken /usr/local/bin/aiken && \
    chmod +x /usr/local/bin/aiken

# Build toml-cli from source
RUN cargo install toml-cli@${TOML_CLI_VERSION} --locked && \
    mv /root/.cargo/bin/toml /usr/local/bin/toml && \
    chmod +x /usr/local/bin/toml

# Install just (pre-built ARM64 binary available)
RUN curl -fsSL "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-aarch64-unknown-linux-musl.tar.gz" -o /tmp/just.tar.gz && \
    echo "${JUST_SHA256}  /tmp/just.tar.gz" | sha256sum -c - && \
    tar -xzf /tmp/just.tar.gz -C /usr/local/bin just && \
    chmod +x /usr/local/bin/just && \
    rm /tmp/just.tar.gz

# Install bun (pre-built ARM64 binary available)
RUN curl -fsSL "https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-linux-aarch64.zip" -o /tmp/bun.zip && \
    echo "${BUN_SHA256}  /tmp/bun.zip" | sha256sum -c - && \
    unzip -q /tmp/bun.zip -d /tmp && \
    mv /tmp/bun-linux-aarch64/bun /usr/local/bin/bun && \
    chmod +x /usr/local/bin/bun && \
    ln -s /usr/local/bin/bun /usr/local/bin/bunx && \
    rm -rf /tmp/bun.zip /tmp/bun-linux-aarch64

# Clean up Rust toolchain and build dependencies to reduce image size
RUN rustup self uninstall -y && \
    apt-get purge -y build-essential pkg-config libssl-dev && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/*

# =============================================================================
# FINAL STAGE: Select architecture-specific stage and configure runtime
# =============================================================================
# The following FROM uses a variable to select between tools-amd64 and tools-arm64
# stages defined above. This is valid BuildKit syntax for multi-arch builds.
ARG TARGETARCH
# kics-scan ignore-line
FROM tools-${TARGETARCH} AS final

# Create non-root user for security
RUN useradd -u 10001 -m -s /bin/bash appuser

WORKDIR /contracts
RUN chown -R appuser:appuser /contracts

USER appuser
WORKDIR /tmp/contracts
RUN echo 'alias ll="ls -lha"' >> ~/.bashrc

# One-shot container: compilation completes and exits, healthcheck is a no-op
HEALTHCHECK --interval=60s --timeout=5s --retries=1 --start-period=5s CMD exit 0

ENTRYPOINT ["/bin/bash", "/compile-contracts.sh"]
