#node
# Restore the devnet chain-spec that live devnet actually runs

Devnet was reset to a ledger 9 genesis on 2.1.101 in the private repo. The
port to `main` (2739f5b2f) regenerated `chain-spec*.json` with main's runtime, so a node built from
`main` computes a different genesis than live devnet and can't join it.

Restores the three chain-spec files from the 2.1.101 release, so `res/devnet/` matches the chain
devnet runs (`chain-spec-hash.json` already pointed at it). This lets a node built from `main` join
devnet as it runs today.
