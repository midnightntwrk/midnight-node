# Deterministic simulation testbed

Runs the real Midnight pallets over the real ledger host functions and a real parity-db ledger
arena inside `sp_io::TestExternalities`, on a virtual clock, so weeks of blocks replay in
seconds and every run is reproducible. Properties are checked against independent oracles
rather than the code's own validation.

```bash
# everything (the ledger arena is process-global, so use one test thread)
cargo test -p midnight-simulation --release --features mock-proofs -- --test-threads=1

# committee rotation only: no ledger arena, no mock-proofs
cargo test -p midnight-simulation --test committee_rotation
```

`--features mock-proofs` builds the ledger with contract and DUST proof verification off
(zswap proofs are always verified), so transactions from `midnight-node-toolkit --proof-server
mock` are accepted. Never enable it outside tests. The same node feature drives
`scripts/tests/microdao-mock-node-e2e.sh`.

## Building blocks

| module | what it is |
|---|---|
| `SimNode` (`src/lib.rs`) | `pallet-midnight`, `pallet-midnight-system` and `pallet-cnight-observation`; `advance_block` drives the timestamp inherent and the ledger's block time like a produced block |
| `cardano` | a forkable simulated Cardano chain of cNIGHT events, and a follower with delayed visibility, bursts and leaked or hidden rollbacks |
| `model` | the cNIGHT observation rules restated from the bridge specification |
| `invariants` | ledger-state invariants read from the state's data structures |
| `committee` | `pallet-session-validator-management` + `pallet-session` on a virtual epoch counter, with the mainchain selection data per block under test control |

## Properties

`tests/cardano_observation.rs`, against `model::ObservationModel`:

| property | checks |
|---|---|
| `runtime_agrees_with_model` | mapping, position, generating set and emitted DUST events match the model for any history and release schedule; the ledger root changes iff a DUST system tx was applied |
| `leaked_rollback_is_rejected_iff_position_regresses` | a leaked fork is rejected with `CardanoPositionRegression`, state untouched, exactly when the model says the position regresses |
| `stable_follower_hides_shallow_rollbacks` | with stability depth `k` > rollback depth, runs with and without the rollback are identical |
| `determinism_and_byte_exact_inherents` | identical streams give identical traces; a reordered or re-positioned inherent fails `check_inherent` |
| `same_height_fork_is_not_detected_by_position_check` | documents that monotonicity ignores the block hash: bridge safety rests on the follower's stability depth |
| `second_inherent_in_a_block_is_rejected` | `InherentAlreadyExecuted` |
| `over_capacity_inherent_is_rejected` | `TooManyUtxos` above `MAX_UTXO_COUNT` |

`tests/committee_rotation.rs`:

| property | checks |
|---|---|
| `committee_follows_the_selection_data` | with stable data, each epoch's committee is the one the selection data named two epochs earlier |
| `boundary_without_stable_data_keeps_the_committee` | **spec, ignored:** a boundary slot without stable data still produces a block and keeps the committee |
| `boundary_without_stable_data_is_lost_today` | current behaviour: that slot panics in `create_inherent` and the next one recovers |
| `rotation_is_deterministic` | identical schedules serve identical committees |

`tests/transactions.rs` (`mock-proofs` only) submits toolkit-built transfers through
`pallet-midnight` (`validate_unsigned`, then dispatch) and applies the same bytes to a toolkit
`LedgerContext` replayed from the same genesis block. Ledger state roots must match byte for byte
after every transaction and block, the invariants must hold, and visible NIGHT must stay constant.

| property | checks |
|---|---|
| `genesis_roots_agree` | roots match at genesis and after an empty block |
| `runtime_and_toolkit_agree` | 1–3 transfers across blocks dispatch Ok, roots agree, and the sender loses exactly what it sent |
| `validate_unsigned_agrees_with_dispatch` | the pool verdict is Ok iff dispatch is Ok, for applicable and conflicting transfers |
| `double_spend_never_moves_funds` | a guaranteed-segment double spend is rejected with the root unchanged; a fallible one dispatches as a partial success that charges the fee but moves no NIGHT |
| `replay_is_rejected` | resubmitting a transaction in the same or next block is rejected without touching the root |
| `dispatch_is_deterministic` | two nodes fed the same sequence produce identical roots |

## Invariants

| invariant | statement |
|---|---|
| `zswap_tree_dense` | `first_free` equals the number of shielded commitments |
| `zswap_root_known` | at a block boundary, the commitment-tree root is in the root history |
| `zswap_nullifiers_bounded` | never more nullifiers than commitments |
| `utxo_ids_unique` | no two live unshielded UTXOs share (intent hash, output number) |
| `utxo_values_positive` | no live UTXO has value 0 |
| `dust_root_known` | at a block boundary, the DUST commitment root is in the DUST root history |
| `dust_nullifiers_bounded` | never more DUST nullifiers than DUST commitments |
| `night_within_supply` | visible NIGHT never exceeds `MAX_SUPPLY` |

`invariants::night_conserved` states supply conservation for flows that move NIGHT across the
shielded boundary.
