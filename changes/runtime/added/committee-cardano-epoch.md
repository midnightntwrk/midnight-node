#runtime #node #partner-chains #migrations

# Record on each committee the Cardano epoch whose stake snapshot selected it

`CommitteeInfo` gains `mc_epoch`: the Cardano epoch `E` whose stake snapshot selected the
committee's members, the one holding the first slot of its Midnight epoch. The selection data comes
from `E − 2`; a block produced by the committee belongs to `E`.
The committee selection inherent data (`/ariadne`) changes format: from
`SessionValidatorManagementApi` version 3 it is `SelectionInherentData`, the selection inputs
together with that Cardano epoch. `set` stores the epoch with the selected committee,
and a verifying node rejects a block whose value differs from the one it derives. A committee kept
because no new one could be selected keeps the Cardano epoch of its members. The value travels with
the committee through every rotation.

The node reads the API version of the runtime that will execute the block (in the on-chain
context, so a runtime upgrade staged in the parent block counts) and provides the data in that
runtime's format, so a node released with this change keeps working with the current runtime.
Release the node before the runtime: once the runtime is upgraded, a node without this change
provides the old format, which the runtime refuses with an error naming the cause.

Storage version 3 re-encodes the stored committees with no Cardano epoch; they rotate out within
two epochs. The version 2 migration now writes the version 2 shape through storage aliases, so both run in
order on chains still at version 1.

PR: https://github.com/midnightntwrk/midnight-node/pull/2505
