#runtime #bridge

# Keep subminimal Reserve transfers on the reserve distribution path

`handle_incoming_transfer` compared the transfer amount against the ledger's
`c_to_m_bridge_min_amount` before looking at the recipient, so a `Reserve` transfer
below the minimum went into the subminimal accumulator and was eventually flushed
with `UnlockToTreasury` instead of `DistributeReserve`.

PR: https://github.com/midnightntwrk/midnight-node/pull/2293
Issue: https://github.com/midnightntwrk/midnight-node/issues/2292
