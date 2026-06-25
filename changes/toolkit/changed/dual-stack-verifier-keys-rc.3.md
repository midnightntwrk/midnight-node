#toolkit
# Load v1-circuit contract verifier keys into the ledger-9 dual-stack v2 slot

Under ledger 9-rc.3 `ContractOperation` is dual-stack: v1 (zk-stdlib v1) circuits
verify their proofs (`ProofVersioned::V2`) against the `v2` slot, which holds a 2.x
`transient_crypto_old` verifier key tagged `verifier-key[v6]`; v2 circuits use the
`v3` slot (3.x `transient_crypto`, `verifier-key[v7]`).

The simple-merkle-tree and counter test contracts are v1 circuits, so their stored
verifier keys are 2.x keys. The toolkit's deploy path loaded them via the 3.x
`verifier_key()` / `contract_operation_new()`, which could not deserialize the v6
keys and produced an operation with no verifier key — contract deploys failed with
`VerifierKeyNotSet { operation: check }`.

Add a `verifier_key_v1` loader (deserializes as 2.x) and a per-generation
`contract_operation_new_v1`: pre-ledger-9 it is the existing single-stack path;
under ledger 9 it places the 2.x key in `ContractOperation::v2`. The merkle-tree
contract deploy now uses these. Verified end-to-end (deploy -> store -> check), with
the `check` call's proof verifying against `op.v2_vk()`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1738
Issue: https://github.com/midnightntwrk/midnight-node/issues/1737
