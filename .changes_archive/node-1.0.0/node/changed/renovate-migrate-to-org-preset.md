#sre
# Migrate Renovate to org-wide hardened preset

Replace interim hardening config with the shared `github>midnightntwrk/renovate-config`
preset. Delegates supply chain hardening (7-day cooldown, strict internal checks
filter, OSV vulnerability scanning, major version gating) to the org preset.
Retains Earthfile custom manager and git-submodules support.

PR: https://github.com/midnightntwrk/midnight-node/pull/1118
JIRA: https://shielded.atlassian.net/browse/SRE-2078
