#toolkit #build
# Build compactc from the compact submodule for local development

Adds the Compact compiler source as a git submodule (`compact/`,
LFDT-Minokawa/compact pinned to the 0.31.0 release commit `b5675ec`) and builds
`compactc` from it via nix, instead of relying solely on the prebuilt-binary
download.

`just compactc` builds the compiler and writes a `COMPACT_HOME` wrapper, with the
version-locked `zkir`/`zkir-v3` baked onto `PATH` so an unrelated `zkir` cannot be
picked up. `.envrc` exports `COMPACT_HOME` when the wrapper exists. toolkit-js
`fetch-compactc` and `run-compactc` already honour `COMPACT_HOME`, so they use the
locally built compiler and skip the download.

Note: the upstream tag `compactc-v0.31.0` is mislabelled (its commit already carries
compiler version `0.31.101`), so the submodule is pinned to the actual `0.31.0`
release commit `b5675ec` instead of the tag.

CI builds compactc from the submodule too: the `+compactc-bundle` Earthly target runs
`scripts/build-compactc.sh` inside a `nixos/nix` image (IOG cache enabled, sandbox off)
and emits a self-contained `COMPACT_HOME` bundle consumed by `toolkit-js-prep`, the
toolkit image, and `build-test-toolkit`. The prebuilt-binary download (and the dead
`/compactc-bin` CI artifact) are removed. `toolkit-js-prep` asserts the built compiler's
version equals `COMPACTC_VERSION`, so a submodule bump without a `COMPACTC_VERSION` bump
fails loudly.

PR: https://github.com/midnightntwrk/midnight-node/pull/1662
