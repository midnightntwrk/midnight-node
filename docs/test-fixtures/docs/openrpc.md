# OpenRPC API Specification

The Midnight node exposes a machine-readable API specification via the `rpc.discover` JSON-RPC method, following the [OpenRPC v1.4](https://open-rpc.org/) standard and [EIP-1901](https://eips.ethereum.org/EIPS/eip-1901) convention.

## Querying the API specification

Call `rpc.discover` on a running node to retrieve the full OpenRPC document:

```bash
curl -s -X POST \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"rpc.discover","id":1}' \
  http://localhost:9944 | jq .result
```

The response is a JSON object containing every RPC method the node supports, including parameter types, return types, error definitions, and descriptions.

## Static specification

A static copy of the specification is available at [`docs/openrpc.json`](openrpc.json) for offline use. This file is regenerated from `rpc.discover` output and kept in sync via CI tests.

## What the specification covers

The OpenRPC document describes three categories of methods:

| Category | Methods | Coverage |
|----------|---------|----------|
| **Midnight custom** | `midnight_*`, `systemParameters_*`, `network_*` | Full — parameter schemas, return types, error codes, descriptions |
| **Partner-chain** | `sidechain_*` | Full — same coverage as custom methods |
| **Standard Substrate** | `system_*`, `chain_*`, `state_*`, `author_*`, `grandpa_*`, `mmr_*`, `beefy_*` | Reference — method names listed with pointers to upstream Substrate documentation |

## Using the specification

### OpenRPC Playground

Paste the contents of `docs/openrpc.json` (or the `rpc.discover` response) into the [OpenRPC Playground](https://playground.open-rpc.org/) to browse the API interactively.

### Client code generation

The [OpenRPC Generator](https://github.com/open-rpc/generator-client) can produce typed client libraries from the specification:

```bash
npx @open-rpc/generator-client \
  --document docs/openrpc.json \
  --language typescript \
  --output ./generated-client
```

Supported languages include TypeScript, Rust, Python, and Go.

### Postman collection

A Postman collection can be derived from the OpenRPC document using [openrpc-to-postman](https://github.com/open-rpc/openrpc-to-postman) or by importing the JSON into tools that support OpenRPC.

## Drift detection

Tests at two levels verify that the specification stays in sync with the node:

**Unit tests (run in CI automatically):**
- **Method count tests** — verify custom (16) and standard Substrate (52) method counts match expected totals
- **Static file sync test** — ensures `docs/openrpc.json` matches the document produced by `build_openrpc_document()`
- **No duplicates test** — verifies no duplicate method names exist in the document

**Integration test (requires a running node):**
- **`rpc_discover_matches_rpc_methods`** — connects to a running node, calls both `rpc_methods` and `rpc.discover`, and verifies every method the node actually serves appears in the OpenRPC document. This catches methods registered in code but missing from the OpenRPC metadata.

```bash
# Run against a local node on port 9944:
cargo test -p midnight-node --lib openrpc::tests::rpc_discover_matches_rpc_methods -- --ignored

# Run against a different endpoint:
RPC_URL=http://node.example.com:9944 cargo test -p midnight-node --lib openrpc::tests::rpc_discover_matches_rpc_methods -- --ignored
```

If a method is added or removed without updating the specification, the unit tests will fail in CI. The integration test provides an additional runtime verification layer.

## Regenerating the static file

To update `docs/openrpc.json` after modifying RPC methods:

```bash
cargo test -p midnight-node test_regenerate_openrpc_json -- --ignored
```

This runs the ignored generator test which writes the current `rpc.discover` output to `docs/openrpc.json`.
