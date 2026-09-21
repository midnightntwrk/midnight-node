#toolkit #ecdsa #hardfork
# Accept `ecdsa:` seeds on chains forked from ledger 8

The ECDSA guard checked the ledger version of the first block, so every chain with ledger-8
history refused `ecdsa:` seeds even after the hard fork to ledger 9. The guard now checks the
chain tip.

Ledger 8 cannot represent an ECDSA NIGHT key, so before the fork such a seed gets a *watch-only*
unshielded sub-wallet at its ECDSA address — no key material is derived for it, and in particular
not the seed's Schnorr key, which is a separate identity at a separate derivation path
(`m/44'/2400'/0'/0/0` vs `.../4/0`). Its shielded and dust sub-wallets are real and replay
normally, and the ECDSA key material is installed at the fork block, at the same address the
pre-fork wallet was already watching.

That matters because the shielded address is scheme-independent — `ecdsa:<seed>` and `<seed>`
share one — so shielded funds received before the fork stay visible and spendable after it.

A chain still on ledger 8 keeps the clear refusal, and it is now checked over every requested
seed before the replay starts, so a wallet-cache entry cannot wave an unsupported chain through.

ECDSA wallet-cache entries are rebuilt once, since a cached ECDSA wallet's shielded state now
includes its pre-fork history. Schnorr caches are unaffected.

PR: https://github.com/midnightntwrk/midnight-node/pull/2181
PR: https://github.com/midnightntwrk/midnight-node/pull/2183
Issue: https://github.com/midnightntwrk/midnight-node/issues/2180
