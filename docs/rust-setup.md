---
title: Prerequisites & Setup
---

## Prerequisites

Midnight-node is built with the Rust programming language on top of the Polkadot SDK. The build and test processes also rely on containerized build systems and task runners.

### 1. Rust Toolchain

For detailed installation instructions for Rust and Polkadot SDK dependencies, please refer to the official Polkadot SDK documentation:

**[Install Polkadot SDK Dependencies](https://docs.polkadot.com/develop/parachains/install-polkadot-sdk/)**

This repository includes a `rust-toolchain.toml` file that specifies the exact Rust version to use. The toolchain will be automatically installed when you run any `cargo` command.

To verify your Rust installation:

```bash
rustup show
```

### 2. Earthly (Containerized Builds)

Earthly is required for building Docker images, regenerating metadata, and rebuilding genesis state.

**macOS:**
```bash
brew install earthly
```

**Ubuntu/Debian:**
```bash
sudo wget https://github.com/earthly/earthly/releases/latest/download/earthly-linux-amd64 -O /usr/local/bin/earthly
sudo chmod +x /usr/local/bin/earthly
```

**Windows (WSL2):**
Install via the Ubuntu instructions above inside your WSL2 terminal. Ensure Docker Desktop is configured to use the WSL2 backend.

To verify Earthly:
```bash
earthly --version
```

### 3. Just (Command Runner)

`just` is required for running end-to-end (E2E) tests and toolkit compilation.

**macOS / Linux / Windows:**
```bash
cargo install just
```

To verify Just:
```bash
just --version
```

## Environment Setup

### Option A: Direnv (Recommended)

The repository includes an `.envrc` file for environment configuration. You can use direnv to automatically load environment variables when entering the directory:

```bash
# Install direnv (macOS)
brew install direnv

# Install direnv (Ubuntu/Debian)
sudo apt install direnv

# Add to your shell (~/.bashrc or ~/.zshrc)
eval "$(direnv hook bash)"  # or zsh, fish, etc.

# Allow direnv in the repository
cd /path/to/midnight-node
direnv allow
```

### Option B: Nix (Alternative)

If you prefer Nix, the repository provides a Nix flake that sets up all dependencies (Rust, Earthly, Just) automatically in an isolated environment.

```bash
# Start the Nix development shell
nix develop
```

*Note: You still need to load environment variables via `direnv allow` or by manually sourcing `.envrc` after entering the Nix shell.*

### Option C: Manual

If you don't want to use direnv or Nix, source `.envrc` manually before running commands:

```bash
source .envrc
cargo check
cargo test
```

## Verify Setup

After completing the setup, verify everything works by running the basic development commands:

```bash
# Check cargo commands work
cargo check

# Run tests
cargo test

# Check earthly targets
earthly doc
```

For troubleshooting common setup or build issues, see [Troubleshooting](troubleshooting.md).
