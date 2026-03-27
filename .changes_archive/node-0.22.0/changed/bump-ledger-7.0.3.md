#client #ledger

# Bump ledger 7 to 7.0.3-rc.1 and ledger 8 to 8.0.0-rc.5

Bump ledger 7 from 7.0.2 (crates.io) to 7.0.3-rc.1 (git tag) and ledger 8
from 8.0.0-rc.4 to 8.0.0-rc.5. Fixes diamond dependency on shared crates
(base-crypto, coin-structure, etc.) by giving each ledger version its own
dependency tree. Concretizes fork module generics with concrete Db7/Db8 types.

PR: https://github.com/midnightntwrk/midnight-node/pull/816
JIRA: https://shielded.atlassian.net/browse/PM-22040
