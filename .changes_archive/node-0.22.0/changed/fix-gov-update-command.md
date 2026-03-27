# Fix governance update command in ephemeral environments

Fixes parsing of large WASM files when attempting the upgrade. Remove hard-coded threshold for proposal calls from ephemeral env script.

PR: https://github.com/midnightntwrk/midnight-node/pull/517
JIRA: https://shielded.atlassian.net/browse/PM-21136