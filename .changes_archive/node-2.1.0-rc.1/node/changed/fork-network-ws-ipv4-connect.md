# Fix fork-network runtime upgrade hang connecting to the forked node

The `fork-network` workflow's `runtime` upgrade mode could hang until the
45-minute job timeout at "Connecting to node at ws://localhost:9950". The forked
chain is healthy the whole time (all validators up, RPC bound, producing and
finalizing blocks) — the problem is that on some self-hosted runners the host
loopback -> docker-published-port path is black-holed (e.g. Docker started with
`userland-proxy` disabled): the SYN is silently dropped, so the polkadot-js
`WsProvider` — which has no connect timeout of its own — never establishes and
never errors. `image` mode is unaffected because it never opens a `WsProvider`.

The `runtime` step now brings the fork up first, then connects to whichever RPC
endpoint actually answers — preferring the published port but falling back to
node1's docker bridge IP, which is routable from the runner regardless of the
loopback/DNAT setup — and reuses that endpoint for the finality-wait and
`:code` verification steps. As defense-in-depth, `createApi` in the
local-environment tooling now fails fast with an actionable error after a bounded
connect timeout (`API_CONNECT_TIMEOUT_MS`), and its `DEFAULT_RPC_URL` uses an
explicit IPv4 host. `full` mode
brings the fork up internally (no `--skip-run`) so it still relies on the
published port; making it robust needs a follow-up tooling change.

PR:
Issue:
