#toolkit
# Pay DUST fees from the largest outputs first

The toolkit paid a transaction's DUST fee by walking the wallet's DUST outputs in storage
order, which is by nullifier, taking what it needed from each. A wallet that has received
many small NIGHT UTXOs holds as many small DUST outputs. When its large output sorted late,
the fee was drawn from dozens of small ones, and every extra spend raised the fee being
paid. The fee loop then gave up with `Could not balance TX`, although the wallet held far
more DUST than the fee. Outputs are now spent largest first, so a single spend usually
covers the fee.

PR: https://github.com/midnightntwrk/midnight-node/pull/2344
