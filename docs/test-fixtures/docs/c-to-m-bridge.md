# Cardano-to-Midnight bridge

The Cardano-to-Midnight (C2M) bridge conceptually allows transferring NIGHT from Cardano to Midnight.

> **Want to exercise it hands-on?** See the
> [C-to-M bridge Happy Path walkthrough (Stagenet)](./c-to-m-bridge-walkthrough.md)
> for step-by-step commands to lock cNIGHT, observe it on Midnight, and claim mNIGHT.
> To check whether the bridge is switched on for a chain, or to enable it, see
> [Enabling the C-to-M bridge](./c-to-m-bridge-enabling.md).

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
  - These are `Invalid Transfers`.
- Unapproved Transfers (a special case of `User Transfers`; does not apply to `Reserve Transfers`)
  - The bridge stores a list of approved Cardano transaction hashes; when a user transfer is detected, its source Cardano transaction hash is checked against this allow-list.
  - Transfers reflecting an unapproved Cardano transaction are credited to the Midnight Treasury.
  - This is a temporary solution that is planned to be removed.
  - These are `Unapproved Transfers`.

## Operating the bridge

C2M bridge is a part of the protocol of Midnight network. There has to be a consensus across nodes regarding observed Cardano events and their reflection in the Midnight chain.
All configuration values that enable the bridge are read from ledger and needs to be set for the bridge to become active.

The configuration values are:

A) main chain (Cardano) scripts data (addresses)

- native token policy id (`0x0691b2fecca1ac4f53cb6dfb00b7013e561d1f34403b957cbb5af1fa` for mainnet, it is policy of NIGHT token)
- native token asset name (`NIGHT`/`0x4e49474854` for mainnet)
- ICS Validator Address (`addr1wyczfpxfnf5hvp36mrn655ye4k2cwluvlez6phx8jx46k6s2ttdaq` for mainnet)
- Reserve Validator Address (`addr1w950c5zxn5fhwlauvpy3ssk287q0qlwz6e2zc4gaj62vaxsy3s9p0` for mainnet)

B) data checkpoint (for idempotency)

- Cardano block number or Cardano Transaction Hash (both are valid pointers to Cardano ledger) after which observability will look for bridge transactions.
  It depends on the point of Cardano history after which we want the transactions to be included.

Both A) and B) are set with the same extrinsic called `setMainChainScripts` of the `bridge` pallet.
This extrinsic has to be submitted by Midnight governance.

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
  - allows addition of approved Cardano Transactions hashes
  - cleans up observed hashes
- for each transfer it executes a Midnight Ledger operation (with the exception of subminimal transfers — many are required for one Midnight operation) and emits events for the indexer

## Assumptions

In order to calculate transfer amounts two assumptions are made:

- Reserve Validator allows to spend cNIGHT only to ICS
- ICS Withdrawals are not part of the same transaction as locking tokens in ICS
