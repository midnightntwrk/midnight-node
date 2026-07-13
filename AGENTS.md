# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Project Overview

Midnight Node is a Substrate-based blockchain implementation for the Midnight network - a privacy-preserving blockchain with zero-knowledge proof capabilities. It operates as a Cardano Partner Chain with integration to the Cardano mainchain.

## Build Commands

**Daily development:** `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt`, `cargo build --release`

**Run specific test:** `cargo test test_name` or `cargo test -- --nocapture` for output

**Earthly commands:**
```bash
earthly -P +rebuild-metadata              # Update runtime metadata
earthly -P +rebuild-chainspec --NETWORK=<network>  # Rebuild chainspec for network
earthly -P +rebuild-all-chainspecs        # Rebuild all chainspecs
earthly -P +rebuild-genesis-state-<NETWORK>  # Rebuild genesis for specific network
earthly -P +rebuild-all-genesis-states    # Rebuild all network genesis states
earthly +node-image                       # Build node Docker image
earthly +toolkit-image                    # Build toolkit image
earthly doc                               # List all available targets
```

**GitHub PR bots:** Comment on a PR to trigger rebuilds:
- `/bot rebuild-metadata` - Rebuild runtime metadata
- `/bot rebuild-chainspec <network1> <network2>` - Rebuild chainspecs for specified networks
- `/bot cargo-fmt` - Run cargo fmt

**E2E tests (just):**
```bash
just toolkit-e2e <NODE_IMAGE> <TOOLKIT_IMAGE>
```

**Genesis generation:**
```bash
./scripts/genesis/genesis-construction.sh  # Interactive genesis construction wizard
```
See [Genesis Construction Guide](docs/genesis/construction.md) for complete documentation.

**Genesis verification:**
```bash
./scripts/genesis/genesis-verification.sh  # Interactive genesis verification wizard
```
See [Genesis Verification Guide](docs/genesis/verification.md) for complete documentation.

## Architecture

```
/node/         - Main node binary, CLI, RPC server, service initialization
/runtime/      - Substrate runtime assembly, pallet configuration
/pallets/      - Custom runtime pallets:
  ├── midnight             - Core ledger state and transaction execution
  ├── midnight-system      - System transaction management (root privileges)
  ├── cnight-observation   - Cardano bridge (cNIGHT to DUST token bridging)
  ├── federated-authority  - Multi-collective governance system
  ├── federated-authority-observation - Governance sync from mainchain
  └── version              - Runtime version tracking in block digests
/primitives/   - Shared types and runtime interfaces (7 crates)
/ledger/       - Midnight ledger types and state management
/res/          - Chain specifications and network configuration
/res/cfg/      - Config presets per network
/util/toolkit/ - Transaction generator for testing
/tests/e2e/    - End-to-end test suite
```

**Consensus:** AURA (6-second blocks) + GRANDPA (finality) + BEEFY (bridge security)

**Key dependencies:**
- `midnight-ledger` - Privacy ledger with zero-knowledge proofs
- `polkadot-sdk` - Substrate framework
- `partner-chains` - Cardano sidechain framework

## Development Setup

```bash
source .envrc  # Load environment with direnv
cargo check
```

See `docs/rust-setup.md` for Rust toolchain installation.

**Running a local node** (always use release mode):
```bash
cargo build --release
CFG_PRESET=dev ./target/release/midnight-node
```
Ports: P2P 30333, RPC 9944

**Compiling contracts locally (compactc):** The Compact compiler source is vendored
as the `compact/` git submodule (pinned to the 0.31.0 release commit). Build it once
with nix and expose it to toolkit-js:
```bash
git submodule update --init compact
just compactc   # builds compactc, writes the COMPACT_HOME wrapper
```
`.envrc` then exports `COMPACT_HOME`, so `cd util/toolkit-js && npm run compact` uses
the locally built compiler instead of downloading the prebuilt binary. Re-run
`just compactc` after bumping the submodule. First build compiles `zkir` from source
unless you have nix `trusted-users` access to the IOG cache (`cache.iog.io`).

`COMPACTC_VERSION` selects which compiler CI uses. It can take one of three forms:
- `<compiler-version>-<12-char-tree-hash>` (e.g. `0.31.0-6587676a9bb2`) — the pinned
  `compact/` submodule, produced by `scripts/compact-submodule-version.sh` (the compiler
  version comes from the submodule's `flake.nix`, the hash is `git rev-parse HEAD^{tree} | cut -c1-12`).
  Regenerate it after bumping the submodule: `scripts/compact-submodule-version.sh > COMPACTC_VERSION`.
- `<compiler-version>-<40-char-commit-sha>` (e.g. `0.31.110-3a289c2e7811d2868e7810bd5a5f1f0b7055995f`)
  — a public **dev build** published from an arbitrary `compact` commit. Set this by hand to pin a
  dev build without touching the submodule.
- a plain or pre-release version (e.g. `0.31.108`, `0.30.0-rc.1`) — a conventional release.

`+node-ci-image` decides build-vs-fetch by comparing `COMPACTC_VERSION` against the live
submodule version (computed `LOCALLY`, since the COPY'd submodule has no `.git`):
- **Match** (the tree-hash suffix agrees) → build from source via `+compactc-bundle`, which runs
  `scripts/build-compactc.sh` inside a `nixos/nix` image (IOG cache enabled) and emits a
  self-contained `COMPACT_HOME` bundle.
- **No match** → fetch the prebuilt binary via `+compactc-fetch`. It picks the release by suffix:
  a bare 40-char hex commit SHA selects the dev build (`compactc-dev-<sha>` tag /
  `compactc_dev-<sha>_<arch>…` asset); anything else (plain version or semver pre-release like
  `-rc.1`) uses the conventional `compactc-v<version>` tag / `compactc_v<version>_<arch>…` asset.

Either way the image then asserts the resulting `compactc --version` equals the
`COMPACTC_VERSION` prefix (the compiler reports the bare semver, no suffix), so a submodule bump
without regenerating `COMPACTC_VERSION` fails loudly. The full suffixed `COMPACTC_VERSION` is also
used in the CI/toolkit image tags.

**Debugging ledger issues:** Keep a local checkout of `midnight-ledger` for searching error messages and understanding `LedgerState` implementation.

**Recommended tools:**
- [gh CLI](https://cli.github.com/) - GitHub CLI for creating PRs, viewing issues, etc.

**Troubleshooting WASM runtime builds (Linux):** If `cargo check` / `cargo build` fails inside `secp256k1-sys`'s build script with `error: call to undeclared library function 'memmove'` (and similar `-Wimplicit-function-declaration` errors), your clang is newer than the bundled `wasm/wasm-sysroot/string.h` expects. Demote the warning back to non-fatal — for example in `~/.cargo/config.toml`:

```toml
[env]
CFLAGS_wasm32v1_none = "-Wno-error=implicit-function-declaration"
CFLAGS_wasm32_unknown_unknown = "-Wno-error=implicit-function-declaration"
```

The symbols still resolve at link time from the real wasi/libc pulled in by the Substrate runtime build.

## When to Rebuild

**Metadata** (use `/bot rebuild-metadata` on PR, or `earthly -P +rebuild-metadata`):
- Pallet storage items change
- Extrinsic signatures change
- Runtime APIs are added/modified

**Genesis** (`earthly -P +rebuild-genesis-state-<NETWORK>`):
- Genesis code changes in toolkit
- Genesis seeds change
- New ledger version

## Network Configurations

Config presets are in `res/cfg/`:
- `dev` - Local development (no AWS secrets required)
- `qanet` - QA testing network
- `preview` - Preview/staging network
- `preprod` - Pre-production network

Networks other than `dev` require AWS access for genesis rebuilds. Contact the node team if you need help.

## Git Workflow

**Branching:** Always create a new branch for changes - never push directly to main. Branch names should be prefixed with a short name moniker (e.g., `jill-my-feature`).

**Commit messages:** Must follow [Conventional Commits](https://www.conventionalcommits.org/) format:
- `feat:` new features
- `fix:` bug fixes
- `chore:` maintenance tasks
- `docs:` documentation changes
- `refactor:` code refactoring
- `test:` adding/updating tests

**Mark commits and PRs if AI-assisted**
- DO NOT use Co-authored-by lines for LLM tools (e.g., `Co-Authored-By: Claude <noreply@anthropic.com>`)
  - Instead: append "Assisted-by: AGENT_NAME:MODEL_VERSION"
  - Example: "Assisted-by: Claude:claude-4.7-opus"
- When creating PRs, add the `ai-assisted` label

**No force pushing:** Never use `git push --force` or `git push --force-with-lease`. Reviewers must re-review all commits after force pushes, and it disrupts the review process. If you need to make corrections:
- Create a new commit with the fix (e.g., `chore: fix typo in change file`)
- This applies especially to change files - if you need to add a PR link after creating the PR, make a separate commit

**Signed commits:** All commits must be signed. Use `git commit -S` or configure Git to sign commits by default with `git config commit.gpgsign true`.

**Creating PRs:** Use `gh pr create` to create pull requests. Always fill in the PR template (see `.github/pull_request_template.md`) - use `--body` with a heredoc that includes all template sections. Leave human-only checkboxes unchecked (e.g., "Self-reviewed the diff" - only humans can self-review). Prompt the user for a relevant GitHub issue link and add labels as needed:
- No issue link: add `-l skip-changes-check-issue`
- No change file: add `-l skip-changes-check-all`

## Change Files

PRs that affect the node, toolkit, or runtime should include a change file in the appropriate component subdirectory:

- `changes/node/added/` or `changes/node/changed/` — for node changes
- `changes/toolkit/added/` or `changes/toolkit/changed/` — for toolkit changes
- `changes/runtime/added/` or `changes/runtime/changed/` — for runtime changes

Format:

```
#tag1 #tag2
# Short description of the change

Longer description of the change

PR: <link to PR>
Issue: <link to Github Issue, if applicable>
```

Change files are optional for changes that don't affect products (e.g., CI-only changes), but are worth adding for significant changes anyway.

## GitHub Actions Workflows

When writing or modifying GitHub Actions workflows (`.github/workflows/*.yml`):

- **Never interpolate `${{ }}` expressions directly in `run:` blocks.** Pass them via `env:` and reference as `"$ENV_VAR"` in the script. This prevents shell injection from untrusted context data.

  ```yaml
  # Bad — shell injection risk
  run: |
    if [ "${{ inputs.my-input }}" = "true" ]; then ...

  # Good — safe
  env:
    MY_INPUT: ${{ inputs.my-input }}
  run: |
    if [ "$MY_INPUT" = "true" ]; then ...
  ```

- **`if:` conditions on steps are safe** — `if: inputs.skip-node != true` is evaluated by the Actions runner, not interpolated into shell.
- **Job-level `if:` on reusable workflow calls uses string comparison** — use `github.event.inputs.skip-node != 'true'` (quoted string), not boolean.
- **Use `!cancelled() && !failure()`** on downstream jobs to let them run when upstream jobs are skipped (but not when failed).

## License Header

See `LICENSE_HEADER.txt` for the required header on all new source files.

## Local Rules

Personal or environment-specific rules can be defined in `AGENTS.local.md` (gitignored):

@AGENTS.local.md
