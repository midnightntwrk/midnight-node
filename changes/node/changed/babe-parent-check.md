#node #consensus #babe

# Check BABE blocks against their parent header

Before BABE verification, the node now rejects a BABE block whose number does not follow its
parent's, or whose parent has no BABE pre-digest.

PR: https://github.com/midnightntwrk/midnight-node/pull/2515
