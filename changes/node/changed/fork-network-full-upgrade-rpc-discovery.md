# Make fork-network full-upgrade mode resolve a reachable RPC endpoint

Follow-up to the `runtime`-mode fix for the loopback -> docker-published-port
black-holing seen on some self-hosted runners. The `fork-network` workflow's
`full` upgrade mode previously ran the all-in-one `full-upgrade:<network>`
command, which brings the fork up internally and always targets the host
published port (`ws://127.0.0.1:9950`); on a runner where that path is
black-holed the governance submission hangs until the job timeout.

`full` mode now runs its two constituent phases directly so the same RPC
endpoint discovery used by `runtime` mode can run in between: `image-upgrade`
(full-upgrade's phase 1 — snapshot restore, bring-up, and client image
rollout), then a probe that prefers the published port but falls back to node1's
docker bridge IP, then `governance-runtime-upgrade --skip-run` (phase 2) against
whichever endpoint answered. The discovered endpoint is also exported for the
finality-wait and `:code` verification steps. The decomposition is
behaviour-identical to `full-upgrade` and completes the robustness work called
out as a follow-up in the runtime-mode fix.

PR:
Issue:
