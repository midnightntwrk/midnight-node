# Using bridged tokens in Compact contracts

Research assessment, 22 October 2026. This answers whether the native
unshielded token in the [Snowbridge proposal](snowbridge-runtime-ledger-integration-spike.md)
can be received, inspected, held, and spent by Compact contracts.

**Yes at the language and ledger design level, provided the bridge issues a
real Midnight unshielded asset. No new Compact language feature appears
necessary.** Existing operations take a token's 32-byte color and do not require
the consuming contract to have minted that asset. A bridge balance held only in
a FRAME pallet or an application map would not automatically satisfy these APIs.

This is a source-backed feasibility result, not an executed bridge-to-contract
test. Issuance, client replay, transaction funding, and compatible tooling remain
implementation requirements. The important distinction is between Compact
supporting the operation and our complete stack successfully constructing and
executing the funded transaction.

The [end-to-end testing strategy](snowbridge-e2e-testing-strategy.md) incorporates
the consumer fixture into a full deposit → ordinary contract use → redemption
scenario, with exact balances, segment checks and finalized ledger replay.

The [draft v1 specification](snowbridge-implementation-spec.md) adds a concrete
escrow sketch and bounded accessor contract. It identifies the empty-committee,
threshold-one maintenance setting and requires checking empty initial redemption
state as well as exact operation/verifier identity before registry activation.
The compiled state layout and funded fixture remain outstanding.

## What a Compact application can do

| Application capability | Existing operation | Qualification |
| --- | --- | --- |
| Identify the asset | Store or compare its `Bytes<32>` color | Pin the bridge's canonical color; a token name or Ethereum address alone is insufficient |
| Accept payment or collateral | `receiveUnshielded(color, amount)` | The transaction must supply matching value; this is not an ERC-20 allowance-based pull from a wallet |
| Hold the bridged asset | Ledger-maintained contract balance | Ordinary application state can record entitlements separately, but cannot substitute for funding |
| Inspect its own balance | `unshieldedBalance(color)` | Reads the contract's balance at the start of execution |
| Require sufficient liquidity | `unshieldedBalanceGte(color, amount)` and related comparisons | Usually preferable to depending on an exact balance |
| Pay a user | `sendUnshielded(color, amount, right<ContractAddress, UserAddress>(recipient))` | Requires sufficient contract funding and application authorization |
| Transfer to another contract | `sendUnshielded` with a `ContractAddress` recipient | Requires coordinated transaction effects and the receiving action; it is not an automatic asynchronous callback |
| Redeem to Ethereum | Fund the proposed bridge redemption escrow and record the request | Runtime export, Snowbridge proofs, and Ethereum release are subsequent steps |

These APIs exist in the inspected Compact 0.34.0 standard library. The current
official reference also documents receipt, sending, and contract-balance
queries. For exact signatures and semantics, prefer the pinned library over
unversioned documentation. [Pinned library][compact], [official API][api]

There is no need for an ordinary consuming contract to import Snowbridge,
verify Ethereum headers, or call `mintUnshieldedToken`. The runtime verifies the
bridge authorization before issuance; subsequent applications use normal
Midnight asset semantics. They still inherit the bridge's backing and security
risks when accepting that asset.

## Why runtime issuance can be compatible

The inspected ledger represents an unshielded token as
`UnshieldedTokenType(HashOutput)`. A user UTXO contains the value, owner, token
type, intent hash, and output number. It has no separate bridge-origin flag or
requirement to route later spends back through a minting contract.
[Token type][coin], [UTXO structure][structure]

On ordinary spending, `UtxoState::apply_offer` checks membership of the exact
input UTXO and consumes it. Transaction verification accounts for wallet inputs,
wallet outputs, contract receipts, and contract sends by token type and segment.
Contract execution adds receipts to the contract balance and subtracts sends,
with checked arithmetic. This is the existing path the new serialized bridge
issuance must feed. [Ledger verification][verify], [ledger application][semantics]

The consumer's address is used to namespace **minting**, not to restrict which
colors it may receive or send. Minting derives a color from the minting contract
and a domain separator. Receiving and sending take an already defined color.
Consequently, an unrelated application cannot reproduce the bridge color just
by calling its own mint function with the same domain separator.
[Color derivation][contract-address], [standard library][compact]

The runtime adapter must still create valid native outputs with the registered
color and compatible serialization/events. Simply adding an ERC-20 balance to
`pallet-assets`, or writing a number in the bridge pallet, does not make those
funds available to `receiveUnshielded` or `unshieldedBalance`.

## Asset identity and visibility

Configure each application with the canonical bridge color, or a validated
mapping obtained through an explicitly designed registry integration. If the
bridge uses the proposed contract-address/domain derivation, calculate the
same value in Rust and Compact and keep a fixed cross-language test vector.
The repository already provides a
[token-type derivation command](../../util/toolkit/src/commands/show_token_type.rs).

Do not calculate `tokenType(domain, kernel.self())` in each consuming contract
and expect the same bridge asset: each different contract address changes the
result. Use the configured issuer/namespace address or pass the already pinned
color. Do not allow an arbitrary caller to choose the accepted color for a
deposit that creates a valuable application entitlement.

The following are separate kinds of visibility:

- **Contract balance:** available through the existing kernel APIs for a known
  color. `CallContext` receives the executing contract's ledger balance.
- **Application accounting:** ownership of deposits, orders, collateral, and
  withdrawal permissions must be implemented by the application itself.
- **Wallet/UI visibility:** depends on wallet and indexer support for the new
  system issuance event and ledger version. Ticker, decimals, logos, and the
  Ethereum asset mapping need metadata configuration; they are not supplied by
  the balance query.
- **Other wallets or Ethereum state:** the APIs above do not expose arbitrary
  wallet balances, Ethereum vault balances, or Snowbridge pallet storage to a
  Compact circuit. Unverified witness/RPC data is not an authenticated bridge
  status check. Add a verified interface separately if an application needs it.

Unshielded assets remain public even when a contract uses private witnesses or
zero-knowledge conditions to decide how to spend them. Shielded and unshielded
types are distinct ledger variants, even when their underlying color bytes
match. Automatic conversion is not part of this proposal. [Token variants][coin]

## Balance queries and transaction composition

`unshieldedBalance` is a snapshot from the beginning of the current execution.
Calling `receiveUnshielded` and then reading the balance does not return the
new total. Exact-balance queries also constrain application: a transaction can
fail if the balance changed between construction and execution. Use threshold
comparisons where the contract only needs a minimum balance, and explicitly
track within-call arithmetic where needed. [Library implementation][compact]

Receiving funds declares a ledger effect that must be balanced by real inputs.
It does not initiate a transfer from an arbitrary user's account. The payer
must provide and authorize the transaction's UTXO inputs, or another contract
must supply matching value. Sending to a second contract requires compatible
receiver effects in the composed transaction; merely specifying its address
does not execute its business logic.

Keep payment and its application effect in the same successful segment. In
particular, a redemption record must not survive without its funded receipt.
Do not infer payment from a successful outer extrinsic alone because ledger
transactions can partially succeed. Application authorization is also separate:
ledger balancing does not make an unrestricted withdrawal circuit safe.
[Balance and effects checks][verify]

Receive/send APIs and contract balances use `Uint<128>`, but that does not mean
every possible `u128` amount is safely transactible. The inspected transaction
balance accumulator uses checked `i128` conversions and sums. Start with small
bounded amounts and test aggregate limits rather than relying on the API width.

## Existing repository support and the remaining tooling gate

The repository's
[minter test contract](../../util/toolkit-js/test/minter_contract/minter.compact)
already contains `receiveAndSendUnshielded`: it receives a configured color and
sends it to a user. This demonstrates the intended API composition, but its
permissive configuration/mint endpoints are test scaffolding, not safe bridge
or application authorization.

The Rust
[custom contract builder](../../util/toolkit/src/tx_generator/builder/builders/ledger_9/contract_custom.rs)
also has a concrete funding path. It loads the funding wallet's unshielded UTXOs,
accepts explicit `--input-utxo` selections, and builds offers alongside contract
actions. Its segment assignment checks whether a selected UTXO's token and
entire value match a fallible receipt effect. For the initial fixture, use one
exact-value UTXO and one receipt. Do not assume arbitrary input splitting,
change, multiple deposits, and segment assignment work without tests.
[Builder arguments](../../util/toolkit/src/tx_generator/builder/mod.rs)

This supports a controlled toolkit PoC even before confirming the chosen browser
wallet/SDK path. The current official token tutorial shows generic wallet
balancing, but it does not demonstrate `receiveUnshielded`, so it is not evidence
that our exact wallet-to-contract flow works. Validate the selected client
versions with a real funded call. [Official tutorial][tutorial]

The node specifies compiler `0.34.0`, and its corresponding toolkit variant
declares Compact.js `2.5.5-rc.8` and compact-runtime `0.19.0-rc.0`. However, the
installed binary at
`util/toolkit-js/node_modules/@midnight-ntwrk/midnight-js-compact/managed/compactc`
reported **`0.26.108`** during this assessment. No matching 0.34.0 compilation
was performed. Resolve the compiler and runtime versions explicitly before
treating a compile/simulator result as evidence for the node's target stack.
[Version configuration](../../COMPACTC_VERSION),
[toolkit variant](../../util/toolkit-js/compact-0.34.0/package.json)

## Required proof of contract usability

Make ordinary application interaction an explicit acceptance criterion, in
addition to the bridge's special redemption escrow:

1. Build a small consumer contract with a fixed bridge color, a funded deposit
   circuit, balance predicates, and a properly authorized payout circuit.
   Compile and prove it with the target compiler/runtime versions.
2. Credit a wallet through the proposed serialized bridge issuance transition.
   A synthetic ledger fixture may validate native-asset behavior first, but
   label it separately from a real Snowbridge-authorized deposit.
3. Deposit an exact-value input of 10 base units into the consumer. Check that
   the wallet input is spent and the contract holds 10 units.
4. In a subsequent call, confirm the contract can read/compare that balance.
   Pay 4 units to another user under the contract's authorization rules; check
   the contract retains 6. This establishes that an application distinct from
   the bridge can hold and use the asset.
5. Return the remaining 6 to an authorized user, then fund the immutable bridge
   redemption escrow and complete its Ethereum release. Test direct composed
   contract-to-escrow transfer as a separate extension.
6. Replay all system and ordinary transactions in the node/toolkit/indexer
   stack and compare state roots and balances after restart.

Negative cases: missing input, wrong color, wrong amount, double-spent input,
unauthorized payout, partial transaction failure, and stale exact-balance
queries. A successful compilation or a simulator with injected balances does
not pass these ledger acceptance tests.

The research conclusion is therefore **feasible with existing Compact asset
operations, conditional on the native ledger bridge adapter and funded
transaction integration**. A compiler feature is not the identified blocker;
end-to-end issuance, client compatibility, and transaction construction are.

[compact]: https://github.com/LFDT-Minokawa/compact/blob/1f671fc27818df2b2676b3a97f85b2b821756243/compiler/standard-library.compact
[api]: https://docs.midnight.network/compact/standard-library/exports#receiveunshielded
[coin]: https://github.com/midnightntwrk/midnight-ledger/blob/48a978d7fc4342e8f5536eb95f47139477e1a0e8/coin-structure/src/coin.rs
[structure]: https://github.com/midnightntwrk/midnight-ledger/blob/48a978d7fc4342e8f5536eb95f47139477e1a0e8/ledger/src/structure.rs
[verify]: https://github.com/midnightntwrk/midnight-ledger/blob/48a978d7fc4342e8f5536eb95f47139477e1a0e8/ledger/src/verify.rs
[semantics]: https://github.com/midnightntwrk/midnight-ledger/blob/48a978d7fc4342e8f5536eb95f47139477e1a0e8/ledger/src/semantics.rs
[contract-address]: https://github.com/midnightntwrk/midnight-ledger/blob/48a978d7fc4342e8f5536eb95f47139477e1a0e8/coin-structure/src/contract.rs
[tutorial]: https://docs.midnight.network/tokens/unshielded-token
