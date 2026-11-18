#toolkit #security
# Harden arithmetic in coin selection with checked operations

Replace unchecked arithmetic in shielded and unshielded coin selection with `checked_add`, `checked_mul`, and `checked_sub`. Overflow now returns structured errors instead of panicking or wrapping silently. Adds boundary-value unit tests for overflow scenarios. Addresses Least Authority audit finding.

Closes: #1010
PR: https://github.com/midnightntwrk/midnight-node/pull/1293
JIRA: https://shielded.atlassian.net/browse/PM-22018
