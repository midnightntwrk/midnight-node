# Fix fork-network runtime/full upgrade hang on WS connect to localhost

The `fork-network` workflow's `runtime` and `full` upgrade modes could hang
until the 45-minute job timeout at "Connecting to node at ws://localhost:9950".
Under the Node 17+ resolver order `localhost` resolves to IPv6 `::1` first; when
the runner's IPv6 loopback path to the published RPC port is not routable, the
polkadot-js `WsProvider` (which has no connect timeout of its own) retries that
address forever instead of falling back to IPv4. The forked chain is healthy the
whole time — only the client cannot attach — so `image` mode, which never opens
a `WsProvider`, was unaffected.

The workflow now passes explicit IPv4 hosts (`127.0.0.1`) in the `--rpc-url`
and curl `RPC_URL` values. In the local-environment tooling, `createApi` now
fails fast with an actionable error after a bounded connect timeout
(`API_CONNECT_TIMEOUT_MS`) instead of hanging indefinitely, and its
`DEFAULT_RPC_URL` is switched to `127.0.0.1` so the default path cannot hit the
same trap.

PR:
Issue:
