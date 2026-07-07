#toolkit #security #dependencies
# Bump vitest to 3.2.6 to fix critical vulnerability blocking release image scans

The toolkit SBOM vulnerability scan fails on GHSA-5xrq-8626-4rwp (Critical,
"When Vitest UI server is listening, arbitrary file can be read and executed")
in vitest 3.2.4, which is baked into the toolkit image via toolkit-js
devDependencies. This blocked the `Publish multi-arch image` job on
release/node-1.0.1, so the content-hash-tagged multi-arch manifest was never
created and the Create Release workflow failed with "image not found".

Bumps vitest 3.2.4 -> 3.2.6 (the patched release on the 3.x line).

PR: <link to PR>
