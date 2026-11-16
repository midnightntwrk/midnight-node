#node #cnight #security

# cNIGHT observation queries use a total order

The registration, deregistration and asset-spend queries behind the cNIGHT
observation inherent now sort by a total order: spent outputs are ordered by their
source transaction hash and output index, not only by the spending transaction's
position. A query's `LIMIT` can fall inside one Cardano transaction's rows, and
with ties left unresolved, two honest nodes with the same db-sync data could keep
different rows and disagree on the inherent.
