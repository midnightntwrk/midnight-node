#node #runtime
# Implements handler for C-to-M brige

Updates bridge to emit events.
Updates call by adding McTxHash to each transfer.
Updates handler API: handler is expected to return a value that is attached to events.

Implements the handler in Midnight runtime.

PR: https://github.com/midnightntwrk/midnight-node/pull/1188
Required for https://github.com/midnightntwrk/midnight-node/issues/1083
