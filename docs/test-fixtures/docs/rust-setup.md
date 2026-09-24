---
title: Installation
---

## Prerequisites

Midnight-node is built with the Rust programming language on top of Polkadot SDK.

For detailed installation instructions for Rust and Polkadot SDK dependencies, please refer to the official Polkadot SDK documentation:

**[Install Polkadot SDK Dependencies](https://docs.polkadot.com/develop/parachains/install-polkadot-sdk/)**

This guide covers all the necessary build dependencies for different operating systems (Ubuntu, macOS, Windows via WSL, etc.).

## Rust Toolchain

This repository includes a `rust-toolchain.toml` file that specifies the exact Rust version to use. The toolchain will be automatically installed when you run any `cargo` command.

To verify your Rust installation:

```bash
rustup show
```

## Midnight-Specific Setup

### Direnv (Optional)

The repository includes an `.envrc` file for environment configuration. You can use direnv to automatically load environment variables:

```bash
# Install direnv
# macOS:
brew install direnv

# Ubuntu/Debian:
sudo apt install direnv

# Add to your shell (~/.bashrc or ~/.zshrc)
eval "$(direnv hook bash)"  # or zsh, fish, etc.

# Allow direnv in the repository
cd /path/to/midnight-node
direnv allow
```

**Manual alternative:** If you don't want to use direnv, source `.envrc` manually before running commands:

```bash
source .envrc
cargo check
cargo test
```

## Verify Setup

After completing the setup, verify everything works:

```bash
# Check cargo commands work
cargo check

# Check earthly is available
earthly --version
```
