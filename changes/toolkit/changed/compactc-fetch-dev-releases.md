#toolkit #build
# Fetch compactc dev builds by commit SHA, and pin one

`compact` can now publish public releases from arbitrary commits ("dev builds"),
tagged `compactc-dev-<40-char-commit-sha>` with assets named
`compactc_dev-<sha>_<arch>-unknown-linux-musl.zip` — i.e. without the
`compactc-v<version>` naming the `+compactc-fetch` Earthly target assumed.

`+compactc-fetch` now picks the tag/asset by inspecting the `COMPACTC_VERSION`
suffix: a bare 40-char hex commit SHA selects the dev build naming, while
anything else (a plain version like `0.31.108` or a semver pre-release like
`0.30.0-rc.1`) keeps the conventional `compactc-v<version>` naming. The
`<version>-<12-char-tree-hash>` submodule form still routes to the
build-from-source path, unchanged.

`COMPACTC_VERSION` is pinned to the dev build
`0.31.108-73ebfbbff78118e77a83fdc99dca352db0020869` (compiler `0.31.108`, already
a supported toolkit-js variant). The `compact/` submodule and its build-from-source
path stay in place; this value simply differs from the submodule's tree hash, so
CI and `just compactc` fetch the dev build instead.

PR:
