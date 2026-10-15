#toolkit #security
# Clear toolkit SBOM critical and npm audit high findings on release/node-1.0.1

Backports the dependency/tooling hygiene fixes needed to make the `+audit` and
toolkit SBOM scan checks pass on the `release/node-1.0.1` branch. These findings
are environmental (newly published advisories against stale lockfiles / a stale
bundled npm), not caused by any product code change.

- Bumped the pinned `npm install -g npm@11.11.0` to `npm@11.18.0` across the
  Earthfile targets (`toolkit-image`, `audit-npm`, `audit-yarn`, `fix-lock-npm`).
  npm 11.18.0 vendors `tar@7.5.19`, clearing the critical toolkit-image SBOM
  finding (GHSA-23hp-3jrh-7fpw, `tar@7.5.9`) plus the other flagged npm-bundled
  packages. Mirrors main #1919.
- `local-environment/package-lock.json`: `npm audit fix` clears 5 high findings
  (axios, brace-expansion, form-data, js-yaml, ws). Supersedes main #1981, which
  only bumped brace-expansion.
- `util/toolkit-js/package-lock.json`: `npm audit fix` clears postcss, undici and
  ws; a `vite` override (`^7.3.6`, within vitest 3.2.6's existing range) clears the
  remaining high without a vitest major bump.

PR: https://github.com/midnightntwrk/midnight-node/pull/1987
