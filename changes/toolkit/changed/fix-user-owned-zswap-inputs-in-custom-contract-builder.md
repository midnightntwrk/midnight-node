#toolkit #contracts #shielded

# Spend caller-owned zswap inputs through the wallet path in `send-intent`

Custom-contract `send-intent` rebuilt every unmatched zswap input as
contract-owned. That broke circuit calls whose `toolkit-js` state actually
contained a caller-owned shielded coin, because the builder skipped the
existing wallet spend path and tried to prove the input under the wrong owner.

The builder now checks whether each encoded zswap input exactly matches a coin
already tracked in the funding wallet's shielded state. Matching coins are
spent via `WalletState::spend()`, while inputs absent from the wallet keep the
existing contract-owned reconstruction. Added offline regression tests for the
exact-coin match and spend behavior.

Issue: https://github.com/midnightntwrk/midnight-node/issues/2092
