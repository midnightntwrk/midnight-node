# Troubleshooting

This guide covers common issues encountered during development and how to resolve them.

## Build and Compilation Errors

### WASM Runtime Build Fails on Linux (`memmove` Error)

**Symptom:**
Running `cargo check` or `cargo build` fails inside `secp256k1-sys`'s build script with the following error:
```
error: call to undeclared library function 'memmove'
```
(and similar `-Wimplicit-function-declaration` errors).

**Cause:**
Your system's `clang` is newer than what the bundled `wasm/wasm-sysroot/string.h` expects, causing these warnings to be treated as fatal errors.

**Solution:**
Demote the warning back to non-fatal. Add the following to your `~/.cargo/config.toml`:

```toml
[env]
CFLAGS_wasm32v1_none = "-Wno-error=implicit-function-declaration"
CFLAGS_wasm32_unknown_unknown = "-Wno-error=implicit-function-declaration"
```
The symbols will correctly resolve at link time from the real wasi/libc pulled in by the Substrate runtime build.

### Build Failures After Ledger Upgrade

**Symptom:**
Compilation errors in `LedgerState::apply_intent()` or new trait requirements missing after upgrading the `midnight-ledger` dependency.

**Solution:**
1. Maintain a local checkout of the `midnight-ledger` repository to inspect the exact `LedgerState` changes.
2. Check the `Cargo.toml` for correct version pinning.
3. Common fixes include adding initialization for new fields in `TransactionContext` or adjusting return types (`Result<T, E>`) for modified API boundaries.
4. Review [development-workflow.md](development-workflow.md#ledger-upgrades) for a complete guide on ledger upgrades.

---

## Earthly and Docker Issues

### Earthly Genesis Rebuild Fails

**Symptom:**
Running `earthly -P +rebuild-genesis-state-*` fails during execution.

**Cause/Solution:**
- **Missing Configuration Files:** Ensure all generated config files (`cnight-config.json`, `ics-config.json`, etc.) have been generated before running the ledger state build.
- **Docker Resources:** Ensure the Docker daemon is running and has sufficient disk space and memory allocated.
- **Invalid Cardano Tip:** The Cardano block hash (`cardano_tip`) used must be a valid 64-character hex string, exist in your configured `db-sync` database, and be recent enough to contain the smart contract data.

---

## Database Connection Issues

### SSL/TLS Connection Errors to Cardano db-sync

**Symptom:**
When generating genesis configs, connecting to PostgreSQL fails with SSL-related errors.

**Cause:**
By default, the genesis construction tool expects a secure (SSL/TLS) connection to the PostgreSQL `db-sync` database.

**Solution:**
The node strictly requires TLS for all PostgreSQL connections. You must configure your local PostgreSQL instance to support TLS. For a quick local setup, you can generate self-signed certificates and enable TLS in your `postgresql.conf`, or use a proxy like `stunnel`. Plaintext connections (and the legacy `ALLOW_NON_SSL` flag) are no longer supported.

---

## Network and Access Issues

### Genesis Rebuild Requires AWS Secrets

**Symptom:**
Unable to rebuild genesis for deployed networks (`qanet`, `preview`, `testnet`).

**Cause:**
You do not have the required AWS credentials locally, which hold the node keys and wallet seeds.

**Solution:**
- For local, isolated development, you can rebuild for the `undeployed` or `local` network which does not require AWS secrets. (Note: Running a node with `CFG_PRESET=dev` automatically uses the `undeployed` genesis).
- If you absolutely need genesis rebuilt for a deployed network, open a PR with your changes and ask the node team in Slack: *"Could someone with AWS access run `earthly -P +rebuild-genesis` after downloading the secrets?"*
