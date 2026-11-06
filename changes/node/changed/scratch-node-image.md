#node #docker #security
# Build the node image `FROM scratch`

The node image now holds only `/midnight-node`, `/aiken-deployer`, the shared libraries they were linked against (copied from the build image), CA certificates, `/res`, the runtime WASM under `/artifacts-<arch>/`, and `/node/chain` owned by uid 10001. There is no shell, package manager, curl, jq, strace, vim or bytehound, which leaves an attacker with a foothold nothing to work with and removes OS packages from image CVE scans.

Breaking for anything that relied on a shell inside the image:
- The entrypoint is `/midnight-node` itself. `/entrypoint.sh` is gone; the node creates its own base path.
- `docker exec … sh` and `--entrypoint bash`/`cat` no longer work. Copy files out with `docker create` + `docker cp`, or mount a static busybox, as local-env does.
- The `HEALTHCHECK` now runs `midnight-node health`, a new subcommand that exits 0 when `127.0.0.1:$RPC_PORT/health` answers 200.
- The image no longer sets `GIT_CONTENT_HASH_SHORT`, `IMAGE_TAG` or `IMAGE_TAG_DEV`, and no longer ships `/version` or `/bin/.envrc`.

PR: https://github.com/midnightntwrk/midnight-node/pull/2378
Issue: https://github.com/midnightntwrk/midnight-node/issues/2379
