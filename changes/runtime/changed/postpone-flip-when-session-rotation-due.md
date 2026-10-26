#runtime #consensus #babe #migration

# Postpone the AURA→BABE flip when a session rotation is due in the flip block

`pallet-consensus-engine` no longer commits the flip in a block that `pallet_session` is about to
rotate. Its `on_initialize` evaluates the runtime's `ShouldEndSession` (`SessionCommitteeManagement`)
through the new `Config::ShouldEndSession` type, logs a warning when a rotation is due at the last
slot of the epoch and leaves `EngineState` at `ScheduledFlip`, so the flip commits at the end of a
later epoch. Until now the flip and the rotation could share a block: after an outage that left the
flip epoch without a block before its last slot, or when a late committee arrived in the block
before. The node seeds BABE epoch 0 from `BabeApi::current_epoch` at the flip block, which names
the committee queued in `NextAuthorities`; a rotation in the same block installed that committee
right away and queued the next one, so the first BABE block installed another set. Epoch 0 was then
signed by a committee every runtime view (`Session::validators`, `pallet_babe::Authorities`, the
committee pallet) had replaced, and the credited committee never signed. A rotation is due at most
once per epoch, so the postponement is bounded. No storage, call or runtime API changes; metadata
is unchanged.

PR: https://github.com/midnightntwrk/midnight-node/pull/2326
