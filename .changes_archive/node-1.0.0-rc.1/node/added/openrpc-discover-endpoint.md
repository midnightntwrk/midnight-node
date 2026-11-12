#client #node #rpc #api
# Add `rpc.discover` endpoint with OpenRPC v1.4 API specification

Registers a standards-compliant `rpc.discover` JSON-RPC method that returns a complete OpenRPC v1.4 document describing the node's API. Enables client code generation, request validation, and developer discoverability without reading source code.

- 16 custom Midnight methods fully documented with parameter types, return types, error definitions, and descriptions
- 52 standard Substrate methods listed as reference entries
- JSON Schema type definitions generated via `schemars` for all RPC response types
- Static `docs/openrpc.json` committed for offline access
- CI drift-detection tests ensure the schema stays in sync with registered methods

Jira: https://shielded.atlassian.net/browse/PM-6402
PR: https://github.com/midnightntwrk/midnight-node/pull/869
