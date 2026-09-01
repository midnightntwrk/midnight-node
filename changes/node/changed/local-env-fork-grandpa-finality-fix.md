#node #local-env

# Fix GRANDPA finality stall when forking a >=2.0.0 network in local-env

Forking a well-known network (e.g. devnet on the 2.0.0 runtime) with the
local-environment tooling brought the chain up and produced blocks via AURA,
but GRANDPA never finalized: `finalized` stayed frozen at the snapshot height
while `best` climbed indefinitely.

Root cause was the mock-authorities image the fork tooling defaulted to. The
`run --from-snapshot` path used `mock-authorities:0f347c5` and the
`fork-network` CI workflow used `368fd98`; both predate the GRANDPA client-side
aux-storage rewrite (mock-authorities PR #77, merged as `4d8d772`). Those older
images only rewrite the runtime `Grandpa` state and rotate the client voter via
a `ForcedChange` digest, which deadlocks the substrate GRANDPA voter on the
2.0.0 runtime: the client voter's set-id freezes below the runtime's
`CurrentSetId`, and no prevotes are ever counted — not even a node's own —
so finality never advances.

Both defaults are bumped to `4d8d772`, which plants the post-rotation voter set
directly in client aux storage with the client set-id equal to the runtime's
`CurrentSetId`, so finality resumes immediately after restart. `4d8d772` still
includes the Council/Technical Committee membership rewrite that the runtime and
full-upgrade modes rely on. The `MOCK_AUTHORITIES_IMAGE` env var and the
`mock_authorities_tag` workflow input still override the default.

This was not caused by the `--num-validators` feature: the reduced mock set was
installed correctly on both the runtime authority set and the client GRANDPA
voter set. Verified by forking devnet (`2.0.0-rc.3`) from a snapshot with
`--num-validators 3` and `4d8d772`, where all three validators finalize and the
client and runtime GRANDPA set-ids match.

PR: <link>
