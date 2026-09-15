#node
# Populate devnet reserve and ICS configs so genesis has a non-empty Locked pool

devnet's `reserve-config.json` and `ics-config.json` carried `utxos: []` / `total_amount: 0`, so the
generated genesis had `locked_pool = 0`. The C-to-M bridge then has no mNIGHT to credit to
recipients and approved User Transfers cannot complete.

Backports the per-network config values from #1675 (the 2.0-line fix for #1674) to the 1.0.300
line: reserve 6,000,000,000.873988 NIGHT, ICS 1,200,000,000 NIGHT. Validator addresses and policy
IDs are unchanged.

The genesis must be regenerated for this to take effect, and devnet reset onto it.

Closes: #2159
