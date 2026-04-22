#node #toolkit
# Incomplete zeroization after conversion to ordinary buffers

Replace ordinary heap strings and byte buffers used for secret material
(seed, derived keys) with zeroizing container types that wipe on drop.
Minimize string-based handling of secrets and explicitly clear temporary
buffers after command execution.

Locations: util/toolkit/src/toolkit_js/mod.rs#L234, L345

Issue: https://github.com/midnightntwrk/midnight-security/issues/53
JIRA: https://shielded.atlassian.net/browse/PM-22034
PR: https://github.com/midnightntwrk/midnight-node/pull/1379
