#toolkit
# Replace unchecked addition in wallet seed increment with checked_add

Replace unchecked `+` operator in wallet seed increment with `checked_add` to
prevent silent overflow wrapping. Overflow now returns an explicit error instead
of producing a colliding seed that could lead to duplicate key derivation.
Addresses Least Authority audit Issue AL.

PR: TBD
JIRA: https://shielded.atlassian.net/browse/PM-20017
