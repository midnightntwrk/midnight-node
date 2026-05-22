#node #toolkit
# Bump vulnerable dependencies

Update rustls-webpki 0.103.4 to 0.103.10 (RUSTSEC-2026-0049: faulty CRL
distribution point matching) and testcontainers 0.25 to 0.27, pulling
astral-tokio-tar 0.5.6 to 0.6.0 (RUSTSEC-2026-0066: insufficient PAX
extension validation).

PR: https://github.com/midnightntwrk/midnight-node/pull/1079
JIRA: https://shielded.atlassian.net/browse/PM-22035
