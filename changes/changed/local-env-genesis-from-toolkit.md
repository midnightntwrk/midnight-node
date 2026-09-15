# local-env builds genesis with the pinned toolkit image

`midnight-setup` is split into three one-shot jobs — `midnight-setup-configs` (patch
`res/local/*` with the deployed Cardano addresses and UTxOs), `midnight-setup-genesis`
(generate the genesis ledger state with `${TOOLKIT_IMAGE}`) and `midnight-setup`
(build-spec, then wait for the D-parameter).

Genesis is now generated at bring-up instead of being read from the committed
`res/genesis/genesis_*_local.mn`. Those blobs carry a version-bound serialization tag and
are rebuilt from the checkout, so starting local-env on a released node image failed with
`expected header tag 'midnight:ledger-state[v13]:', got 'midnight:ledger-state[v18]:'`.
Generating genesis with the toolkit that matches the node image removes that coupling, so
local-env can be started on an older image (verified on `1.0.0`) as well as on a build of
the current checkout, for which the resulting chain spec is unchanged.

`configurations/midnight-setup/genesis-compat/ledger-parameters-legacy-fields.json` fills
fields that older toolkits still require in `ledger-parameters-config.json` (currently
`cost_model.parallelism_factor`); values in `res/local` win, and newer toolkits ignore
what they no longer know.

PR: <link to PR>
