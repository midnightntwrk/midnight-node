# Regenerate preview genesis and chain-spec for non-empty Locked pool

Preview's reserve and ICS genesis configs were empty (total_amount: 0),
which produced an empty Locked pool at genesis and breaks the C-to-M
bridge. Pool balances are set at genesis and cannot be changed at runtime,
so preview requires a reset. Populate the reserve and ICS configs and
regenerate the preview genesis state and chain-spec.

After the reset the Locked pool holds 16,799,999,999,126,012 STARS
(MAX_SUPPLY - reserve - treasury) and the 24,000,000,000 NIGHT supply
invariant holds.

PR: https://github.com/midnightntwrk/midnight-node/pull/1699
Issue: https://github.com/midnightntwrk/midnight-node/issues/1690
