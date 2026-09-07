#node
# Group cNIGHT observation events by Cardano transaction and make row-limited pulls gap-free

The cNIGHT observation data source now builds its inherent from
`CNightGroupedUtxos`, a type whose events enter and leave only in whole
Cardano-transaction units. "Never emit part of a transaction" is now a property
of the type rather than a call-site sort-and-truncate loop, so a partially
emitted transaction (which every validator would then re-derive differently)
cannot be reintroduced by a future edit.

Each of the four observation queries is row-limited independently. A query that
returns its full row limit is only proven complete below the position of its
last row, and previously the loop reported the range as covered up to the
Cardano tip regardless, silently skipping that category's remaining events. The
merged set is now cut at the earliest saturated category's frontier, and the
cursor resumes at the cut instead of the tip. Saturation is judged on raw rows,
before the decoding and address filters drop any, so a query that survives
filtering as far fewer events is still recognised as saturated.

Genesis construction terminates its scan when the cursor stops advancing rather
than when it enters the tip block, which is correct for the capacity cap and the
new frontier cut alike; a cursor that cannot advance anywhere else now fails
with an explicit error instead of looping forever.

Inherent bytes are unchanged unless a category's raw query actually hits its row
limit — the case that previously dropped events. A test reimplements the shipped
truncation loop and asserts the new pipeline produces identical events and
cursor over generated inputs.

PR: https://github.com/midnightntwrk/midnight-node/pull/2102
