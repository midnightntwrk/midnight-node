# Run local-environment natively on arm64

Makes the local-environment stack run fully native on arm64 hosts:

- Adds a `busybox-init` service that copies an arch-matching static busybox from the
  multi-arch `busybox` image into the shared volume, replacing the previously vendored
  amd64-only binary (which crashed `cardano-node-1` with "Exec format error" on arm64
  once the container ran native). Docker resolves the arch, so no per-arch binaries
  are committed.
- Switches `db-sync` to the multi-arch `cardano-db-sync` image and its `platform`
  to `${ARCHITECTURE}`, so it runs native instead of under emulation. This uses a
  temporary branch build (db-sync 13.7.2.1) until a multi-arch release ships.

PR: https://github.com/midnightntwrk/midnight-node/pull/1874
Issue: https://github.com/midnightntwrk/midnight-node/issues/1873
