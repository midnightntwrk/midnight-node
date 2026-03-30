#toolkit
# Replace unchecked arithmetic in offer creation with checked operations

Replace `as i128` casts and unchecked addition in offer delta calculation and
balance accumulation with `TryFrom`, `checked_add`, and `checked_sub`. Overflow
or truncation now returns an explicit `OfferBuildError` instead of silently
producing incorrect values. Addresses Least Authority audit Issue AL.

PR: https://github.com/midnightntwrk/midnight-node/pull/942
JIRA: https://shielded.atlassian.net/browse/PM-20206
