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

CI tests verify that the specification stays in sync with the node's registered methods:

- **Method inventory test** — compares method names in the OpenRPC document against `rpc_methods` output from a running node
- **Static file sync test** — ensures `docs/openrpc.json` matches the document produced by `rpc.discover`
- **Drift detection tests** — verify custom and standard method counts match expected totals

If a method is added or removed without updating the specification, CI will fail.

## Regenerating the static file

To update `docs/openrpc.json` after modifying RPC methods:

```bash
cargo test -p midnight-node test_regenerate_openrpc_json -- --ignored
```

This runs the ignored generator test which writes the current `rpc.discover` output to `docs/openrpc.json`.
