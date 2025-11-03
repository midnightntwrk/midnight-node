# Add `midnight_` prefix to Chain ID to follow Polkadot convention

The convention for Polkadot-SDK-based chains is to prefix the chain ID with the name of the mainchain.

We're now doing this with Midnight - our internal `network_id` string is derived from the chain_id when generating the chain-spec.

This is to keep the network_id length small - it's included in all transactions, and included as a prefix in addresses.

Examples:

- chain_id: `midnight_preview`, network_id: `preview`, address-prefix: `preview`
- chain_id: `midnight`, network_id: `mainnet`, address-prefix: None

PR: https://github.com/midnightntwrk/midnight-node/pull/188
