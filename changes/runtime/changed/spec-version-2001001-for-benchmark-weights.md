#runtime
# Bump spec_version to 002_001_001 so the benchmark weights can be enacted

`feat(runtime): 2.1.0 benchmark weights` (#2160) changed the runtime without raising
`spec_version`, which leaves the new weights unusable on any running chain: weights live in
the runtime WASM, the WASM lives in chain state, and `frame_system`'s `can_set_code` rejects
a runtime whose `spec_version` does not increase (`SpecVersionNeedsToIncrease`). Both
`set_code` and `apply_authorized_upgrade` therefore refuse the 2.1.0 runtime on a chain
already reporting `002_001_000`.

The only way to enact it without this bump is `set_code_without_checks`, which costs more
than it looks: `frame_executive` gates `on_runtime_upgrade` on the version changing, and
afterwards `last_runtime_upgrade` still reads the old value, so the chain cannot report
which runtime it is executing. On a network whose output is comparative benchmark numbers,
a runtime you cannot identify after the fact defeats the purpose of re-benchmarking.

Only `spec_version` moves. `transaction_version` stays 4 — extrinsic encoding is unchanged —
and `authoring_version` stays 1.
