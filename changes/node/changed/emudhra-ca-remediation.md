#node #security #docker

# Bump Amazon Linux CA bundle and base-image digest for eMudhra remediation

The node image now pins `ca-certificates-2025.2.76-1.0.amzn2023.0.4` (the
explicit NVR of the Amazon Linux fix for CVE-2026-113470 / ALAS2023-2026-3306)
and advances the `amazonlinux:2023-minimal` base-image digest to the current
multi-arch index. The previous version-only pin would have matched any release
of `2025.2.76`, so a rebuild could silently retain a pre-remediation release;
pinning the full NVR forces microdnf to install the fixed package.

Mozilla and downstream distributions removed the compromised eMudhra
intermediate CA certificates. After this update, chains that terminate at the
affected roots are no longer trusted by the OS CA bundle used by the node's
non-rustls TLS paths.

PR: https://github.com/midnightntwrk/midnight-node/pull/2327
Issue: https://github.com/shieldedtech/shielded-sre/issues/706
Advisory: https://alas.aws.amazon.com/AL2023/ALAS-2026-3306.html
