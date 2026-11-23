#toolkit #security
# Bump Node.js to 24.22.2 in toolkit images

Node.js 24.18.0, baked into the toolkit images, is affected by CVE-2026-111497
(critical, on the CISA KEV list), which fails the toolkit SBOM scan. Bump
`NODEJS_VERSION` to 24.22.2, and the fork-network workflow's `node-version` with it.
