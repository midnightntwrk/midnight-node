#node #cnight #consensus

# Order observed cNIGHT UTXOs by data variant, gated on `spec_version`

The `Vec<ObservedUtxo>` the node passes to `pallet-cnight-observation`'s
`process_tokens` inherent is now sorted by `header.tx_position` and then by the
`data` variant — `Deregistration`, `Registration`, `AssetSpend`, `AssetCreate` —
instead of by the legacy "created UTXOs before spent UTXOs" rule.

The runtime processes that vector in order, and `handle_create` needs a valid
registration (exactly one mapping for the owner) at the moment it runs. Under
the legacy rule an `AssetCreate` in the same Cardano transaction as a
registration change is processed before the mapping settles, so it silently
produces no DUST generation event and no `UtxoOwners` entry — permanently, for
that UTXO. Ordering all mapping mutations ahead of all asset creates removes
both cases: registering and receiving cNIGHT in one transaction, and rotating a
mapping (which passes through an ambiguous two-mapping state) in one
transaction. Putting removals before additions also stops the rotation from
emitting a transient `Deregistration`/`Registration` pair that leaves the
indexer's `cnight_registrations` row without its backing UTXO reference.

`tx_position` deliberately stays the primary sort key: one inherent's batch
spans a whole observation window, so a UTXO's `AssetCreate` and the
`AssetSpend` consuming it can both land in one batch, and only the real
on-chain Cardano order keeps the create ahead of the spend.

Because the runtime's `check_inherent` compares the whole UTXO vector for
equality, changing the order is consensus-breaking: a node using a different
rule than the block author rejects the block, including on a full sync from
genesis. The rule is therefore gated on
`DATA_ORDERED_UTXOS_SPEC_VERSION`, resolved against the runtime in state at the
parent block — the same runtime that executes the child block and that
`check_inherents` is called against, so the authoring and verifying paths always
agree. Runtimes below the threshold keep the legacy ordering. No runtime,
runtime API or metadata change is needed.

Genesis construction (`generate_cnight_genesis`) passes the `spec_version` of
the runtime the binary was built with, since it replays the same pallet logic;
regenerated genesis states will use the new ordering.

Issue: https://github.com/midnightntwrk/midnight-node/issues/2121
PR: https://github.com/midnightntwrk/midnight-node/pull/2123
