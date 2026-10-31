# local-env supplies config keys that older node images require

Node images older than the checkout could not start in local-env, failing with
`Input("configuration error: config error: missing field \`tblock_correction_offset\`")`.
`default_cfg()` reads `res/cfg/default.toml` from disk at runtime, and local-env mounts the
working tree over the image's own `/res`, so an older binary looks for keys the current
default.toml no longer defines.

`configurations/legacy-node-cfg.env` now supplies those keys as environment variables — a
config source in their own right — and is wired into every service that runs the node binary.
It does not touch the tracked `res/cfg` files, and is a no-op for images built from the
checkout: build-spec output is byte-identical with and without it. Between `node-0.22.1` and
`node-2.1.0`, only `node-1.0.2` needs any (`tblock_correction_offset`,
`tblock_correction_disable_after`).

PR: https://github.com/midnightntwrk/midnight-node/pull/2156

