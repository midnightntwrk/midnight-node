#toolkit #audit
# Enforce EOF on untagged CLI parser path; document ADR-0022 untagged contract

`hex_ledger_untagged_decode` now rejects trailing bytes after the deserialized
value, closing the silent-fallback ambiguity surface flagged by audit
issue #307. `coin_public_decode` and `contract_address_decode` continue to use
untagged decoding per ADR-0022 (wallet keys and addresses use untagged
serialization, with Bech32m HRP playing the role of a tag at the user-facing
boundary). Inline comments above both parsers cite ADR-0022 and PR #853 to
prevent future re-introduction of tagged decoding for these types. Misleading
"failed to parse seed" error messages are corrected to "invalid hex input".

Closes https://github.com/shieldedtech/shielded-security-engineering/issues/307
PR: https://github.com/midnightntwrk/midnight-node/pull/1437
Ticket: https://shielded.atlassian.net/browse/PM-22028
