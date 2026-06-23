#toolkit #build
# Bump compactc 0.31.108 → 0.31.110 (dev build)

`COMPACTC_VERSION` is repinned from the `0.31.108` dev build
(`73ebfbbff78118e77a83fdc99dca352db0020869`) to the `0.31.110` dev build
(`3a289c2e7811d2868e7810bd5a5f1f0b7055995f`), fetched by `+compactc-fetch` /
`just compactc` from
`https://github.com/LFDT-Minokawa/compact/releases/tag/compactc-dev-3a289c2e7811d2868e7810bd5a5f1f0b7055995f`.

The toolkit-js variant workspace is renamed `compact-0.31.108/` → `compact-0.31.110/`
and registered in `SUPPORTED_COMPACTC_VERSIONS` so the resolver's
`<major>.<minor>.<patch>` dispatch selects it. No bundle rebuild was needed: the
vendored `compact-runtime.tgz` is already
`0.16.103-dev.3a289c2e7811d2868e7810bd5a5f1f0b7055995f`, built from the same
`3a289c2e` source commit as the new compiler (compact-js 2.5.3 unchanged). The
`compact/` submodule is left in place; the dev-build SHA suffix differs from its
tree hash, so CI fetches the prebuilt binary rather than building from source.

PR:
