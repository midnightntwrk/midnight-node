---
title: Installation
---

This guide covers the complete setup needed for midnight-node development. For general Polkadot SDK information, see the [official Polkadot SDK docs](https://docs.polkadot.com).

This page will guide you through the **3 steps** needed to prepare a computer for midnight-node development. Since midnight-node is built with [the Rust programming language](https://www.rust-lang.org/) on top of Polkadot SDK, the first thing you will need to do is prepare the computer for Rust development - these steps will vary based on the computer's operating system. Once Rust is configured, you will use its toolchains to interact with Rust projects; the commands for Rust's toolchains will be the same for all supported, Unix-based operating systems.

**Steps:**
1. Build dependencies
2. Rust developer environment
3. Midnight-specific setup (GitHub access, Nix, Direnv)

## Build dependencies

Polkadot SDK development is easiest on Unix-based operating systems like macOS or Linux. The examples
in the [Polkadot SDK Docs](https://docs.polkadot.com) use Unix-style terminals to demonstrate how to
interact with Polkadot SDK from the command line.

### Ubuntu/Debian

Use a terminal shell to execute the following commands:

```bash
sudo apt update
# May prompt for location information
sudo apt install -y git clang curl libssl-dev llvm libudev-dev
```

### Arch Linux

Run these commands from a terminal:

```bash
pacman -Syu --needed --noconfirm curl git clang
```

### Fedora

Run these commands from a terminal:

```bash
sudo dnf update
sudo dnf install clang curl git openssl-devel
```

### OpenSUSE

Run these commands from a terminal:

```bash
sudo zypper install clang curl git openssl-devel llvm-devel libudev-devel
```

### macOS

> **Apple M1 ARM**
> If you have an Apple M1 ARM system on a chip, make sure that you have Apple Rosetta 2
> installed through `softwareupdate --install-rosetta`. This is only needed to run the
> `protoc` tool during the build. The build itself and the target binaries would remain native.

Open the Terminal application and execute the following commands:

```bash
# Install Homebrew if necessary https://brew.sh/
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"

# Make sure Homebrew is up-to-date, install openssl
brew update
brew install openssl
```

### Windows

**_PLEASE NOTE:_** Native Windows development of Polkadot SDK is _not_ very well supported! It is _highly_
recommend to use [Windows Subsystem Linux](https://docs.microsoft.com/en-us/windows/wsl/install-win10)
(WSL) and follow the instructions for [Ubuntu/Debian](#ubuntudebian).
Please refer to the separate
[guide for native Windows development](https://docs.polkadot.com).

## Rust developer environment

This guide uses <https://rustup.rs> installer and the `rustup` tool to manage the Rust toolchain.
First install and configure `rustup`:

```bash
# Install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Configure
source ~/.cargo/env
```

Configure the Rust toolchain to default to the latest stable version, add nightly and the nightly wasm target:

```bash
rustup default stable
rustup update
rustup update nightly
rustup target add wasm32v1-none --toolchain nightly
```

## Test your set-up

Now the best way to ensure that you have successfully prepared a computer for Polkadot SDK
development is to follow the steps in the [official Polkadot SDK tutorials](https://docs.polkadot.com).

## Troubleshooting Polkadot SDK builds

Sometimes you can't get the Polkadot SDK node template
to compile out of the box. Here are some tips to help you work through that.

### Rust configuration check

To see what Rust toolchain you are presently using, run:

```bash
rustup show
```

This will show something like this (Ubuntu example) output:

```text
Default host: x86_64-unknown-linux-gnu
rustup home:  /home/user/.rustup

installed toolchains
--------------------

stable-x86_64-unknown-linux-gnu (default)
nightly-2020-10-06-x86_64-unknown-linux-gnu
nightly-x86_64-unknown-linux-gnu

installed targets for active toolchain
--------------------------------------

wasm32v1-none
x86_64-unknown-linux-gnu

active toolchain
----------------

stable-x86_64-unknown-linux-gnu (default)
rustc 1.50.0 (cb75ad5db 2021-02-10)
```

As you can see above, the default toolchain is stable, and the
`nightly-x86_64-unknown-linux-gnu` toolchain as well as its `wasm32v1-none` target is installed.
You also see that `nightly-2020-10-06-x86_64-unknown-linux-gnu` is installed, but is not used unless explicitly defined as illustrated in the [specify your nightly version](#specifying-nightly-version)
section.

### WebAssembly compilation

Polkadot SDK uses [WebAssembly](https://webassembly.org) (Wasm) to produce portable blockchain
runtimes. You will need to configure your Rust compiler to use
[`nightly` builds](https://doc.rust-lang.org/book/appendix-07-nightly-rust.html) to allow you to
compile Polkadot SDK runtime code to the Wasm target.

> There are upstream issues in Rust that need to be resolved before all of Polkadot SDK can use the stable Rust toolchain.
> [This is the tracking issue](https://github.com/paritytech/polkadot-sdk/issues) if you're curious as to why and how this will be resolved.

#### Latest nightly for Polkadot SDK `master`

Developers who are building Polkadot SDK _itself_ should always use the latest bug-free versions of
Rust stable and nightly. This is because the Polkadot SDK codebase follows the tip of Rust nightly,
which means that changes in Polkadot SDK often depend on upstream changes in the Rust nightly compiler.
To ensure your Rust compiler is always up to date, you should run:

```bash
rustup update
rustup update nightly
rustup target add wasm32v1-none --toolchain nightly
```

> NOTE: It may be necessary to occasionally rerun `rustup update` if a change in the upstream Polkadot SDK
> codebase depends on a new feature of the Rust compiler. When you do this, both your nightly
> and stable toolchains will be pulled to the most recent release, and for nightly, it is
> generally _not_ expected to compile WASM without error (although it very often does).
> Be sure to [specify your nightly version](#specifying-nightly-version) if you get WASM build errors
> from `rustup` and [downgrade nightly as needed](#downgrading-rust-nightly).

#### Rust nightly toolchain

If you want to guarantee that your build works on your computer as you update Rust and other
dependencies, you should use a specific Rust nightly version that is known to be
compatible with the version of Polkadot SDK they are using; this version will vary from project to
project and different projects may use different mechanisms to communicate this version to
developers. For instance, the Polkadot client specifies this information in its
[release notes](https://github.com/paritytech/polkadot/releases).

```bash
# Specify the specific nightly toolchain in the date below:
rustup install nightly-<yyyy-MM-dd>
```

#### Wasm toolchain

Now, configure the nightly version to work with the Wasm compilation target:

```bash
rustup target add wasm32v1-none --toolchain nightly-<yyyy-MM-dd>
```

### Specifying nightly version

Use the `WASM_BUILD_TOOLCHAIN` environment variable to specify the Rust nightly version a Polkadot SDK
project should use for Wasm compilation:

```bash
WASM_BUILD_TOOLCHAIN=nightly-<yyyy-MM-dd> cargo build --release
```

> Note that this only builds _the runtime_ with the specified nightly. The rest of project will be
> compiled with **your default toolchain**, i.e. the latest installed stable toolchain.

### Downgrading Rust nightly

If your computer is configured to use the latest Rust nightly and you would like to downgrade to a
specific nightly version, follow these steps:

```bash
rustup uninstall nightly
rustup install nightly-<yyyy-MM-dd>
rustup target add wasm32v1-none --toolchain nightly-<yyyy-MM-dd>
```

## Midnight-Specific Setup

### GitHub Personal Access Token

Midnight-node depends on private packages that require authentication. Create a GitHub Personal Access Token (classic) with the following permissions:

1. Go to GitHub Settings > Developer settings > Personal access tokens > Tokens (classic)
2. Generate new token with these scopes:
   - `repo` - Full control of private repositories
   - `read:packages` - Download packages from GitHub Package Registry

### Configure Netrc

Add your GitHub credentials to `~/.netrc`:

```bash
machine github.com
login YOUR_GITHUB_USERNAME
password YOUR_GITHUB_TOKEN
```

Set proper permissions:

```bash
chmod 600 ~/.netrc
```

### Docker Authentication

Authenticate Docker with GitHub Container Registry:

```bash
echo $YOUR_GITHUB_TOKEN | docker login ghcr.io -u YOUR_GITHUB_USERNAME --password-stdin
```

### Nix Development Environment

Midnight-node uses Nix for reproducible development environments. The repository includes a `flake.nix` that sets up all required tools (earthly, rustup, clang, etc.).

Install Nix with flakes enabled:

```bash
# Install Nix (if not already installed)
curl -L https://nixos.org/nix/install | sh

# Enable flakes (add to ~/.config/nix/nix.conf or /etc/nix/nix.conf)
experimental-features = nix-command flakes
```

Enter the development environment:

```bash
# Navigate to midnight-node repository
cd /path/to/midnight-node

# Start Nix development shell
nix develop
```

The Nix shell will automatically source `.envrc` and set up all build dependencies.

### Direnv (Alternative to Nix)

If not using Nix, you can manually source the environment configuration:

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

When using direnv, environment variables from `.envrc` are automatically loaded when you enter the directory.

**Manual alternative:** If you don't want to use direnv, source `.envrc` manually before running commands:

```bash
source .envrc
cargo check
cargo test
```

### Verify Setup

After completing the setup, verify everything works:

```bash
# If using Nix
nix develop

# Check cargo commands work
cargo check

# Check earthly is available
earthly --version
```
