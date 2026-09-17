# Runtime upgrade rehearsals can take the wasm from the node image

`governance-runtime-upgrade` and `full-upgrade` no longer require `--wasm` and a
pre-populated `local-environment/artifacts/`. Node images ship the runtime they were
built with under `/artifacts-<arch>/`, so the candidate runtime can come straight from
the image being rolled out:

- `--wasm-from-image <image>` extracts from that image.
- With neither flag, it defaults to `$NEW_NODE_IMAGE`, else `$NODE_IMAGE` /
  `$MIDNIGHT_NODE_IMAGE`, keeping the candidate runtime and the client binary in lockstep.
- `--wasm <path>` still works, and remains the way to submit a blob that is not in an
  image (a release asset, which is the srtool deterministic build).

Extraction uses `docker create` + `docker cp`, so nothing in the image is executed; it
prefers the `*.compact.compressed.wasm` variant, resolves the architecture from the image
itself, and writes into `artifacts/from-image/<image>/` so the existing wasm path sandbox
still applies. The blob is re-extracted on every run, so a moved tag cannot leave a stale
runtime behind. Taking the runtime from the image the network is already running is
detected and warned about, since the candidate then shares its `spec_version`.

PR: https://github.com/midnightntwrk/midnight-node/pull/2156

