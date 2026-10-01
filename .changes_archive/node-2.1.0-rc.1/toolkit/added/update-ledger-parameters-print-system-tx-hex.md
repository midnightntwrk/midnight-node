#toolkit
# Add `--print-system-tx-hex` flag to `update-ledger-parameters`

Build the `SystemTransaction::OverwriteParameters` payload, print it as a `0x`-prefixed
hex string, and exit without submitting the root call. Council and Technical Committee
keys are not required in this mode. Useful for testing governance flows manually
through the Polkadot-JS Apps UI.

PR: https://github.com/midnightntwrk/midnight-node/pull/1473
