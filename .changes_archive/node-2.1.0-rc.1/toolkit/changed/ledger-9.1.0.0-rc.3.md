#toolkit
# Adapt toolkit to ledger 9.1.0.0-rc.3

Three rc.3 ledger changes required toolkit-side adaptation: `ContractOperation`
became dual-stack, `dust_actions` were folded into the intent signing payload,
and ledger 9's prover became `!Send`.

## Dual-stack contract verifier keys

Under rc.3 `ContractOperation` carries two verifier-key slots: `v2` for 2.x
(`transient_crypto_old`) keys verifying v1 / zk-stdlib-v1 circuits, and `v3` for
3.x (`transient_crypto`) keys verifying v2 circuits.

- New deploys: `contract_operation_new` now takes serialized
  `ContractVerifyingKeyBytes` and peeks the serialization tag to pick the slot —
  `verifier-key[v6]` → `op.v2`, `verifier-key[v7]` → `op.v3`. Previously it took a
  single typed 3.x `VerifierKey`, which could not deserialize the v6 keys, so v1
  contracts (the simple-merkle-tree / counter test contracts) deployed with no
  verifier key and failed with `VerifierKeyNotSet { operation: check }`.
- Contract-maintenance inserts: a new `contract_operation_versioned_verifier_key`
  picks the `ContractOperationVersionedVerifierKey` variant per ledger generation
  — pre-ledger-9 → `V3` (2.x key), ledger 9 → `V4` (3.x key in the `v3` slot) —
  replacing the hard-coded `V3` the insert used before.

Verified end-to-end (deploy → store → check), with the `check` call's proof
verifying against `op.v2_vk()`.

- `ledger/helpers/src/lib.rs` (`ContractVerifyingKeyBytes`, per-generation
  `contract_operation_new` / `contract_operation_versioned_verifier_key`)
- `ledger/helpers/src/versions/common/mod.rs` (`verifier_key` loader)
- `ledger/helpers/src/versions/common/contract/{deploy,merkle_tree,mod}.rs`
  (`deploy` now returns `Result<ContractDeploy, io::Error>` so a key that fails to
  deserialize surfaces as an error instead of a silent empty operation)
- `util/toolkit/src/tx_generator/builder/builders/common/contract_maintenance.rs`

## Dust registrations & unshielded offers signed over the assembled intent

rc.3 folds `dust_actions` into `Intent::data_to_sign` (the new
`IntentSigningEnvelope`), so a dust registration's `night_key` / `dust_address` /
`allow_fee_payment` — and the dust spends — are now part of *every* signature on
the intent, including the unshielded offer's input signatures. The toolkit signed
before attaching the dust, so the payloads no longer matched at validation:

- Dust registrations failed with `InvalidDustRegistrationSignature` during genesis
  generation.
- Unshielded offers (signed by `IntentInfo::build`, before `apply_dust` attaches
  the dust) failed with `IntentSignatureVerificationFailure` (e.g. `generate-txs
  batches`).

Both sign paths now assemble the full intent (offers + unsigned dust) first,
compute `data_to_sign` once, then fill in every signature — mirroring the ledger's
own `Transaction::sign`. `apply_dust` additionally re-signs the unshielded offers
over the final payload, recovering their signing keys from the originating
`IntentInfo`.

- `ledger/helpers/src/versions/common/transaction.rs` (`apply_dust`, new
  `DustRegistrationBuilder::build_unsigned`)
- `ledger/helpers/src/versions/common/intent.rs`
  (`BuildIntent::unshielded_signing_keys` accessor)
- `util/toolkit/src/genesis_generator.rs` (`add_dust_actions`)

## `!Send`-safe local and remote proving

Ledger 9's `Resolver::resolve_key` is an RPITIT, which makes `tx.prove` `!Send`,
but the toolkit drives proving on a multi-thread tokio runtime and the prove body
is shared across L7/L8/L9. Both `LocalProvingProvider` and `ProofServerProvider`
now build and drive the prove future inside a `spawn_blocking` closure running a
fresh current-thread runtime, so the `!Send` future never crosses a thread
boundary; `.await`ing the handle yields the calling worker, so N semaphore-bounded
proofs still run in real parallel. To let the closure be `'static`,
`ProofProvider::prove` now takes `resolver: &'static Resolver` and an owned
`CostModel` (was `&Resolver` / `&CostModel`). The ledger-8 remote path stays
`Send` and awaits directly.

- `ledger/helpers/src/versions/common/proving.rs` (`ProofProvider`,
  `LocalProvingProvider`)
- `util/toolkit/src/remote_prover.rs` (`ProofServerProvider`)
- `ledger/helpers/src/versions/common/{transaction.rs}`,
  `util/toolkit/src/genesis_generator.rs`,
  `util/toolkit/src/tx_generator/builder/builders/common/batch_single_tx.rs`
  (call sites pass an owned, cloned `runtime_cost_model`)

## Other

- `util/toolkit/README.md` doc-test example output refreshed for rc.3 (addresses
  and hashes change under the v18 ledger-state encoding).

PR: https://github.com/midnightntwrk/midnight-node/pull/1738
Issue: https://github.com/midnightntwrk/midnight-node/issues/1737
