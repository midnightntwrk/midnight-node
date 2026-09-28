#toolkit #security

# Clear npm audit findings in toolkit-js and local-environment

Backport of the npm-audit fixes already on main so `+audit-nodejs` passes on
this release line:

- `toml` overridden to `^4.3.0` (GHSA-82x6-q7mm-w9cf, GHSA-v5mp-jgw5-2x6j);
  `@effect/cli` pins `^3.0.0`, which cannot reach the patched 4.x
- `nanoid` 3.3.19 (GHSA-2v37-7h3g-55p8)
- `turbo` 2.9.14 (GHSA-3qcw-2rhx-2726, GHSA-hcf7-66rw-9f5r)
- `vitest` 4.1.11 (GHSA-82fw-gwwq-j7x9 in `@vitest/mocker`)
- local-environment: `js-yaml`, `joi`, `@humanfs/node` via `npm audit fix`

PR: https://github.com/midnightntwrk/midnight-node/pull/2145
