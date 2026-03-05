#ci #security
# Migrate secret and input interpolations to env: block indirection
Remediate 7 audit findings (M-F007 through M-F014) by replacing direct ${{ }} expression interpolation in shell run: blocks with step-level env: block indirection across 7 GitHub Actions workflow files. This eliminates shell metacharacter injection and expression injection vectors.

PR: https://github.com/midnightntwrk/midnight-node/pull/850
Ticket: https://shielded.atlassian.net/browse/PM-22118
