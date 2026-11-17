#runtime #rewards
# Attribute every block to its producer

Groundwork for native NIGHT block rewards: the runtime type `BlockProducer`
resolves the producer of the block being executed. It is stateless: no pallet, no
storage, no metadata change.

- The author index comes from `pallet_consensus_engine`'s `FindAuthor` and is
  resolved against `CurrentCommittee`. Callers must read it after `pallet_session`,
  so that an epoch-opening block sees the committee it installs.
- A registered member is credited to its stake pool (`OperatorId::AdaSpo` of the
  pool key hash); a permissioned member to `Producer::Permissioned`.
- BABE import guarantees a resolvable author. If that invariant ever breaks, the
  block has no producer and a defensive failure is reported; the chain keeps
  running.
- New crate `midnight-primitives-block-production` holds `OperatorId` and
  `Producer` (pinned SCALE encodings) and the `BlockProducer` interface.

PR: TBD
