#security #runtime
# Fix unbounded Mappings storage in cnight-observation pallet

`Mappings` storage used an unbounded `Vec<MappingEntry>` per Cardano reward
address with no size cap. A Cardano address could accumulate arbitrarily many
registration entries via repeated UTXOs, inflating storage and decode cost
per key — a potential DoS vector.

Changes:
- Add `MaxMappingsPerAddress` config constant to `pallet-cnight-observation`
- Change `Mappings` storage value from `Vec<MappingEntry>` to
  `BoundedVec<MappingEntry, T::MaxMappingsPerAddress>`
- Wire up `try_push` in `handle_registration`; excess registrations are
  logged as warnings and dropped rather than panicking
- Genesis build converts `Vec` → `BoundedVec` with an explicit bounds check
- Runtime sets `MaxMappingsPerAddress = 10`
- `MaxRegistrationsExceeded` error variant is now reachable

PR: (pending)
