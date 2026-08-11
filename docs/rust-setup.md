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

### 2. Docker or Podman (Container Runtime)

Earthly runs every build target inside a container, so a container runtime must be installed and running **before** you install or use Earthly.

**macOS:**
Install [Docker Desktop for Mac](https://docs.docker.com/desktop/setup/install/mac-install/) and start it, or install [Podman](https://podman.io/docs/installation#macos) and run `podman machine init && podman machine start`.

**Ubuntu/Debian:**
```bash
# Docker
sudo apt-get update && sudo apt-get install -y docker.io
sudo systemctl enable --now docker
sudo usermod -aG docker "$USER"   # log out & back in for group to take effect

# — or Podman —
sudo apt-get update && sudo apt-get install -y podman
```

**Windows (WSL2):**
Install [Docker Desktop for Windows](https://docs.docker.com/desktop/setup/install/windows-install/) and enable the **WSL 2 backend** in Settings → General.

To verify the container runtime:
```bash
docker info   # or: podman info
```

### 3. Earthly (Containerized Builds)

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
Install via the Ubuntu instructions above inside your WSL2 terminal.

To verify Earthly:
```bash
earthly --version
```

### 4. Just (Command Runner)

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

If you prefer Nix, the repository provides a Nix flake that sets up most dependencies (Rust, Earthly) automatically in an isolated environment. Note that **Just** is not included in the Nix flake and still needs to be installed separately (see step 4 above).

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
```

## Verify Setup

After completing the setup, verify everything works by running the basic development commands:

```bash
# Check that the workspace compiles
cargo check

# Check earthly targets
earthly doc
```

> **Note:** Do not run a bare `cargo test` to verify your setup. The
> `midnight-node-toolkit` crate depends on generated npm artifacts that are only
> available after running the toolkit prep step, and some pallet fixture tests
> depend on `.mn` files that must be regenerated with the toolkit. To run the
> core test suite, use Earthly which automatically excludes these:
>
> ```bash
> earthly -P +test --secret DOCKERHUB_USER= --secret DOCKERHUB_TOKEN=
> ```
>
> The `--secret` flags are required even for local runs (empty values use
> anonymous Docker Hub access).
>
> If you prefer running tests with `cargo` directly, exclude the toolkit crate
> and the fixture-dependent tests:
>
> ```bash
> cargo test --workspace --locked \
>   --exclude midnight-node-toolkit \
>   --exclude partner-chains-cardano-offchain \
>   -- --skip tests::test_get_contract_state \
>      --skip tests::test_send_mn_transaction \
>      --skip tests::test_validation_works
> ```

For troubleshooting common setup or build issues, see [Troubleshooting](troubleshooting.md).
