# Run local-environment natively on arm64

Makes the local-environment stack run fully native on arm64 hosts:

- Vendors an arch-specific busybox (`busybox.amd64` / `busybox.arm64`, both static
  BusyBox 1.35.0) mounted into the Cardano containers by `${ARCH}`. The previously
  vendored busybox was an amd64 binary, which crashed `cardano-node-1` with
  "Exec format error" on arm64 once the container ran native.
- Switches `db-sync` to the multi-arch `cardano-db-sync` image and its `platform`
  to `${ARCHITECTURE}`, so it runs native instead of under emulation. This uses a
  temporary branch build (db-sync 13.7.2.1) until a multi-arch release ships.

PR: https://github.com/midnightntwrk/midnight-node/pull/1874
Issue: https://github.com/midnightntwrk/midnight-node/issues/1873
