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
