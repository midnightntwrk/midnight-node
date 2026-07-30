# Toolkit wallet-cache: snapshot GC retention floor and cache observability

The file-backend ledger-snapshot GC now always retains the newest 2 snapshots
even when no cached wallet references their height. Previously, a save whose
per-wallet writes were skipped by `write_wallet_if_newer` (heights not
advancing) left its just-written ledger snapshot unreferenced, and the next GC
deleted exactly the snapshot the following warm start needed.

`set_wallet_states` now reports skipped writes (existing file already at a
same-or-newer height) instead of counting them as saved, and the tx builder
logs a warning when uncached wallet seeds force the replay back to genesis —
previously this fallback was silent and a single stray seed caused a
full-chain replay with no indication why.
