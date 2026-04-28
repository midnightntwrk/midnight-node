#toolkit #audit
# Switch CLI parsers to tagged deserialization and enforce trailing-byte rejection

`coin_public_decode` and `contract_address_decode` now use tagged deserialization
instead of unconditionally falling back to untagged parsing. The untagged fallback
is removed. `hex_ledger_untagged_decode` now rejects trailing bytes after the
deserialized value. Misleading "failed to parse seed" error messages are corrected.

Closes https://github.com/shieldedtech/shielded-security-engineering/issues/307
PR: https://github.com/midnightntwrk/midnight-node/pull/1437
Ticket: https://shielded.atlassian.net/browse/PM-22028
