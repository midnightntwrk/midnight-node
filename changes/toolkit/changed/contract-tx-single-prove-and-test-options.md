#toolkit #performance #testing
# Contract transactions are proved once; options for fast local test chains

Contract calls used to be proved twice, once to size the fee and once for real. Fee sizing now
mock-proves contract calls too, so the real proof is produced once, and local proving runs the
proofs of a transaction in parallel.

New options:
- `--proof-server mock`: mock contract and DUST proofs, accepted only by `mock-proofs` nodes.
- `--fetch-best` / `MN_FETCH_BEST`: sync to the best block instead of the finalized one.
- `--no-wait-finality` / `MN_NO_WAIT_FINALITY`: `send` returns on best-block inclusion.
- `send-intent --tee-file`: also write the transactions to a file while submitting them.
- `send-intent --output-onchain-state`: write the contract state as it will be after the call.

The first three are for chains that cannot reorg, such as local-env.

Property tests build randomly shaped transfers with the toolkit's builder and check them against
the real ledger offline (`tests/tx_properties.rs`) and against a live node
(`tests/tx_properties_live.rs`, gated on `MN_LIVE_NODE_WS`). A `ci-e2e` cargo profile builds
throwaway e2e binaries without debug info.

PR: TBD
