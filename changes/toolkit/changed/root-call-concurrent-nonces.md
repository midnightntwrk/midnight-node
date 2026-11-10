#toolkit
# Let concurrent root calls share governance signers

`root-call` took each transaction's nonce from subxt's default, which reads finalized
state. Two root calls running at once in one process, such as concurrent e2e tests, signed
with the same council and technical committee keys, so their transactions reused a nonce
and one was rejected with `priority is too low`. `root-call` now assigns nonces from the
larger of the node's `system_accountNextIndex` and the next nonce it has already handed
out for that signer in the process.

PR: https://github.com/midnightntwrk/midnight-node/pull/2405
