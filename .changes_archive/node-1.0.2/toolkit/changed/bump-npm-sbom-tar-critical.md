#toolkit #security
# Bump bundled npm 11.11.0 -> 11.18.0 to clear toolkit image SBOM findings

The Grype scan of the toolkit image failed on a critical `tar` advisory
(GHSA-23hp-3jrh-7fpw, `tar@7.5.9`, fixed in 7.5.19). That `tar`, along with the
other flagged npm packages (sigstore, @sigstore/core, @sigstore/verify,
minimatch, brace-expansion, picomatch, ip-address), is vendored inside the
globally-installed npm CLI, not in toolkit-js's dependencies.

- Bumped the pinned `npm install -g npm@11.11.0` to `npm@11.18.0` across the
  Earthfile targets (`toolkit-image`, `audit-npm`, `audit-yarn`,
  `fix-lock-npm`). npm 11.18.0 bundles `tar@7.5.19` (clearing the critical) plus
  patched versions of every other flagged npm package.
- Minor bump within the 11.x line; engine requirement is unchanged
  (`^20.17.0 || >=22.9.0`), satisfied by the image's Node 24.18.0.

PR: https://github.com/midnightntwrk/midnight-node/pull/1919
