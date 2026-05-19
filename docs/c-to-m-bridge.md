# Cardano-to-Midnight bridge

The Cardano-to-Midnight (C2M) bridge conceptually allows transferring NIGHT from Cardano to Midnight.

## Bridge features

- Cardano Reserve to Cardano Locked transactions
  - The bridge computes how much cNIGHT was moved from the Reserve Validator to the Illiquid Circulation Supply Validator (ICS) address; this is the amount of mNIGHT that will be released from the Midnight Reserve to pay block rewards.
  - These are `Reserve Transfers`.
- Cardano Unlocked to Cardano Locked transactions
  - The bridge observes transactions that lock cNIGHT at the ICS from addresses other than the Reserve Validator.
  - The bridge computes the net difference given transaction has at the ICS as the amount of the transfer.
  - The bridge unlocks this amount of mNIGHT, minus fees, from the Midnight Locked pool as a claimable amount for the designated transfer recipient.
  - There is one transfer recipient encoded in the Cardano transaction metadata.
  - These are `User Transfers`.
- Subminimal Transfers (a special case of `User Transfers`; does not apply to `Reserve Transfers`)
  - If a Cardano transaction locks less cNIGHT than fees would consume (the user would not be able to claim anything), the bridge accumulates this amount in internal storage instead of executing a Midnight transaction.
  - Once the accumulated amount exceeds a configurable threshold, the Midnight Treasury is credited with the accumulated amount.
  - These are `Subminimal Transfers Flushes`.
- Invalid Transfers (a special case of `User Transfers`; does not apply to `Reserve Transfers`)
  - Beyond invalid amounts, a transfer can have invalid transaction metadata. The UI should help prevent such cases, but on Cardano nothing prevents the owner of cNIGHT from spending it in an arbitrary transaction.
  - Cardano transactions that lock user cNIGHT at the ICS but lack the expected metadata encoding the recipient address are credited to the Midnight Treasury.
  - These are `Invalid Transactions`.
- Unapproved Transfers (a special case of `User Transfers`; does not apply to `Reserve Transfers`)
  - The bridge stores a list of approved Cardano transaction hashes; when a user transfer is detected, its source Cardano transaction hash is checked against this allow-list.
  - Transfers reflecting an unapproved Cardano transaction are credited to the Midnight Treasury.
  - This is a temporary solution that is planned to be removed.
  - These are `Unapproved Transfers`.

## Implementation details

The complete C2M bridge implementation is split between two pallets and their supporting code.

`pallet-bridge` is the foundation, developed by IOG. Its (and its supporting code's) responsibilities are:

- idempotent Cardano observation and IDP creation
- classification of Cardano transactions into `User Transfers` and `Reserve Transfers`
- it also uses an injected runtime function that parses metadata into a recipient data (public key hash); if this transformation fails, the transfer is classified as an `Invalid Transfer`
- for each transfer it executes a callback

`c2m-bridge-pallet` builds on `pallet-bridge` functionality:

- it defines the callback executed by `pallet-bridge`
- it hosts the subminimal transfers feature:
  - stores the accumulated amount
  - knows the threshold for a minimal valid user transfer and the threshold for flushing the accumulated amount
- it hosts the approved transactions feature:
  - allows addition of approved hashes
  - cleans up observed hashes
- for each transfer it executes a Midnight Ledger operation (with the exception of subminimal transfers — many are required for one Midnight operation) and emits events for the indexer

## Assumptions

In order to calculate transfer amounts two assumptions are made:

- Reserve Validator allows to spend cNIGHT only to ICS
- ICS Withdrawals are not part of the same transaction as locking tokens in ICS
