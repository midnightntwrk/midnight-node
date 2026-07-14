#runtime
# Base `validate_unsigned` DUST validity window on wall-clock next-slot time

`pallet_midnight`'s `validate_unsigned` (tx-pool path) now derives the DUST validity-window
reference time `tblock` from the current wall-clock time rounded up to the next AURA slot
boundary, instead of the last produced block's on-chain timestamp plus a `MaxSkippedSlots`
margin. When block production stalls, the stored timestamp is stale and previously caused
valid transactions to be wrongly rejected with `OutOfDustValidityWindow`; tracking real time
fixes this.

Wall-clock time is supplied through a new pallet `Config` type `WallClockMillis: Get<u64>`,
wired in the runtime to a `wall_clock::now_millis()` host function. This is used only for
pool validation and never for block execution/consensus (which stays on the deterministic
on-chain timestamp via `pre_dispatch`).

The now-unused `MaxSkippedSlots` storage item (and its default) was removed. This is a
metadata change; rebuild runtime metadata.

PR: https://github.com/midnightntwrk/midnight-node/pull/1877
Issue: https://github.com/midnightntwrk/midnight-node/issues/1856
