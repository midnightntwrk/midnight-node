#toolkit
# Fund contract-bound shielded coins from the funding wallet in `send-intent`

`send-intent` (the custom-contract builder) now balances the shielded side of a contract call.
Coins are placed in the offer of the transcript that claimed them: guaranteed transcript into the
guaranteed offer, fallible transcript into the fallible offer of the contract's segment, since the
ledger matches claimed nullifiers/commitments and balances tokens per segment. Per segment and
token type the builder nets the call's outputs against its contract-owned inputs and the tokens it
mints, and covers any remaining deficit by spending the funding wallet's coins in that segment,
with change returned to the wallet. Previously only DUST fees were balanced and all coins were
pinned to fixed segments, so circuits that take a coin from the caller (`receiveShielded(coin)`,
e.g. micro-dao `set_topic`/`buy_in`/`vote_commit`) could not be submitted through the toolkit.

Also adds `scripts/tests/microdao-local-env-e2e.sh` (`just microdao-local-env-e2e`), a timed
end-to-end run of every micro-dao circuit against a fresh local-env.

PR:
Issue:
