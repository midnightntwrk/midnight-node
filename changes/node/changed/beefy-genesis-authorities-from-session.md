#node #genesis #beefy
# Let session seed the BEEFY genesis authorities

Building genesis panicked once beefy became a session key:

```
GenesisBuilder_build_state -> pallet_session::GenesisConfig::build
  -> <(_, _, _) as SessionHandler>::on_genesis_session -> unwrap_failed
```

`pallet_beefy` (pallet index 21) initialized its authorities from the chain
spec's `BeefyConfig::authorities`, and then `pallet_session` (index 30)
initialized them a second time via the now three-element `SessionHandler`.
`pallet_beefy::initialize` returns `Err` when the authority list is already
populated, and `on_genesis_session` turns that into
`.expect("Authorities vec too big")` — a misleading message, since nothing is
too big.

The chain spec now leaves `beefy: Default::default()` (empty authorities,
`genesis_block: Some(1)`), matching `aura`, `babe` and `grandpa` beside it, and
lets `pallet_session` seed the BEEFY authorities from the committee's session
keys. Note this changes genesis storage, and therefore the genesis hash, so it
applies to chains created from this version onward.

PR: https://github.com/midnightntwrk/midnight-node/pull/1953
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
