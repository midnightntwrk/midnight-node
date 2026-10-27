#node #security

# Fix denial of service in the SPO registration and deregistration commands

Registration and deregistration offchain collected every output at the registration
validator address whose datum decoded as a registration with the caller's payment key hash and
stake pool public key, and added each one as a script input with the execution budget from
`Costs::get_one_spend()`. That accessor panics unless the evaluated transaction contains exactly
one spend redeemer, so a second matching output made the command abort.

Both selection criteria are public data and paying to a script address runs no validator, so any
third party could place outputs that matched a given SPO and block its registration, key rotation
and deregistration indefinitely, without holding any of its keys.

Changes:

- Execution budgets are now resolved per spent input. `Costs` records the budget Ogmios returned
  for each input of the evaluated transaction, and the registration transactions look a budget up
  with `CostStore::get_spend_for_input`, which returns an error instead of panicking when the
  evaluation result does not cover an input. This also removes the dependency on redeemer indices,
  which shift with the inputs the balancing step selects.
- A single transaction spends at most 10 registration outputs. Planted outputs are spent like any
  other - their datum names the SPO as the owner, so spending them clears them from the validator
  address and recovers the ADA they hold - but they are ordered last, behind the outputs whose
  stake pool signature verifies. A bounded batch therefore always takes the SPO's genuine
  registrations first, so registration and deregistration take effect in a single transaction no
  matter how many outputs were planted, and the remaining ones are swept by re-running the command.

PR: https://github.com/midnightntwrk/midnight-node/pull/2240
Issue: https://github.com/midnightntwrk/midnight-node/issues/2239
