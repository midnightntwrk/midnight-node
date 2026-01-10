# System Parameters Pallet

The `system_parameters` pallet stores and manages on-chain governance parameters that control critical aspects of the Midnight network. It provides a secure mechanism for updating these parameters through privileged governance origins.

## Parameters

### Terms and Conditions

Stores the network's Terms and Conditions reference:
- **Hash**: SHA-256 hash of the terms and conditions document
- **URL**: Location where the full document can be retrieved (max 256 bytes)

### D-Parameter

Controls the authority selection process:
- **num_permissioned_candidates**: Expected number of permissioned candidates in the committee
- **num_registered_candidates**: Expected number of registered candidates in the committee

## Genesis Configuration

Parameters are initialized at genesis via JSON configuration files located at `res/{network}/system-parameters-config.json`. The configuration uses a nested structure:

```json
{
  "terms_and_conditions": {
    "hash": "0x...",
    "url": "https://..."
  },
  "d_parameter": {
    "num_permissioned_candidates": 10,
    "num_registered_candidates": 0
  }
}
```

## Extrinsics

Both extrinsics require `SystemOrigin` (typically Root or governance origin):

- `update_terms_and_conditions(hash, url)`: Updates the Terms and Conditions
- `update_d_parameter(num_permissioned_candidates, num_registered_candidates)`: Updates the D-Parameter

## Runtime API

The pallet exposes runtime APIs for querying current values:

- `get_terms_and_conditions()`: Returns the current Terms and Conditions (if set)
- `get_d_parameter()`: Returns the current D-Parameter

## RPC Endpoints

JSON-RPC endpoints are available for external queries:

- `systemParameters_getTermsAndConditions`: Returns Terms and Conditions with hex-encoded hash
- `systemParameters_getDParameter`: Returns the D-Parameter values
- `systemParameters_getAriadneParameters(epoch_number, d_parameter_at?)`: Returns Ariadne parameters for a mainchain epoch, with D Parameter sourced from on-chain storage instead of Cardano

### Ariadne Parameters Endpoint

The `getAriadneParameters` endpoint returns the same response schema as `sidechain_getAriadneParameters` but sources the D Parameter from `pallet-system-parameters` on-chain storage. This endpoint should be used instead of the deprecated `sidechain_getAriadneParameters` which reads D Parameter from Cardano.

**Parameters:**
- `epoch_number` (required): The mainchain epoch number to query candidates for
- `d_parameter_at` (optional): Block hash to query D Parameter from. If not provided, uses the best (latest) block. This is useful when querying historical epoch data and you want the D Parameter value that was in effect at a specific block.

**Response includes:**
- **d_parameter**: D Parameter from on-chain pallet storage
- **permissioned_candidates**: Permissioned candidate data from Cardano
- **candidate_registrations**: Registered candidate data from Cardano
- **d_parameter_block_info**: Metadata about the block from which D Parameter was fetched
  - `block_hash`: The block hash used to query D Parameter
  - `block_number`: The block number used to query D Parameter

The `d_parameter_block_info` field ensures transparency about data provenance, especially when mixing historical epoch data from Cardano with on-chain D Parameter values.