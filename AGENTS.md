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
just node-e2e <NODE_IMAGE> <TOOLKIT_IMAGE>
just hardfork-e2e <NODE_IMAGE> <UPGRADER_IMAGE>
just toolkit-e2e <NODE_IMAGE> <TOOLKIT_IMAGE>
```

**Genesis generation:**
```bash
./scripts/genesis/genesis-generation.sh  # Interactive genesis generation wizard
```
See [Genesis Generation Guide](docs/genesis/generation.md) for complete documentation.

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

**Debugging ledger issues:** Keep a local checkout of `midnight-ledger` for searching error messages and understanding `LedgerState` implementation.

**Recommended tools:**
- [gh CLI](https://cli.github.com/) - GitHub CLI for creating PRs, viewing issues, etc.

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
- `node-dev-01` - Single node development
- `qanet` - QA testing network
- `preview` - Preview/staging network
- `preprod` - Pre-production network

Networks other than `dev`/`node-dev-01` require AWS access for genesis rebuilds. Contact the node team if you need help.

## Git Workflow

**Branching:** Always create a new branch for changes - never push directly to main. Branch names should be prefixed with a short name moniker (e.g., `jill-my-feature`).

**Commit messages:** Must follow [Conventional Commits](https://www.conventionalcommits.org/) format:
- `feat:` new features
- `fix:` bug fixes
- `chore:` maintenance tasks
- `docs:` documentation changes
- `refactor:` code refactoring
- `test:` adding/updating tests

**Do not include:**
- LLM watermarks (e.g., "Generated by Claude", "Written by AI", etc.)
- Co-authored-by lines for LLM tools (e.g., `Co-Authored-By: Claude <noreply@anthropic.com>`)

**No force pushing:** Never use `git push --force` or `git push --force-with-lease`. Reviewers must re-review all commits after force pushes, and it disrupts the review process. If you need to make corrections:
- Create a new commit with the fix (e.g., `chore: fix typo in change file`)
- This applies especially to change files - if you need to add a PR link after creating the PR, make a separate commit

**Signed commits:** All commits must be signed. Use `git commit -S` or configure Git to sign commits by default with `git config commit.gpgsign true`.

**Creating PRs:** Use `gh pr create` to create pull requests. Always fill in the PR template (see `.github/pull_request_template.md`) - use `--body` with a heredoc that includes all template sections. Leave human-only checkboxes unchecked (e.g., "Self-reviewed the diff" - only humans can self-review). Prompt the user for a relevant JIRA ticket link and add labels as needed:
- No JIRA link: add `-l skip-changes-check-jira`
- No change file: add `-l skip-changes-check-all`

## Change Files

PRs that affect the node or toolkit images should include a change file. Create a new file in the `changes/added` or `changes/changed` directory with the format:

```
#tag1 #tag2
# Short description of the change

Longer description of the change

PR: <link to PR>
JIRA: <link to JIRA ticket, if applicable>
```

Change files are optional for changes that don't affect products (e.g., CI-only changes), but are worth adding for significant changes anyway.

## License Header

See `LICENSE_HEADER.txt` for the required header on all new source files.

## Local Rules

Personal or environment-specific rules can be defined in `AGENTS.local.md` (gitignored):

@AGENTS.local.md
