#node #security #docker

# Bump Amazon Linux CA bundle and base-image digest for eMudhra remediation

The node image now pins `ca-certificates-2025.2.76-1.0.amzn2023.0.4` (the
explicit NVR of the Amazon Linux fix for CVE-2026-113470 / ALAS2023-2026-3306)
and advances the `amazonlinux:2023-minimal` base-image digest to the current
multi-arch index. The previous version-only pin would have matched any release
of `2025.2.76`, so a rebuild could silently retain a pre-remediation release;
pinning the full NVR forces microdnf to install the fixed package.

Mirrors the equivalent change on `midnightntwrk/midnight-node` so this shadow
remains byte-equivalent with upstream for the OS layer.

Advisory: https://alas.aws.amazon.com/AL2023/ALAS-2026-3306.html
Upstream PR: https://github.com/midnightntwrk/midnight-node/pull/2327
