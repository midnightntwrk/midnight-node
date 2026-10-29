#node #ledger
# Persist ledger state only during block authoring and import

Ledger state is now persisted and flushed only during block authoring and block
import, which the node's executor marks (`CallContext::Onchain`). Other runtime
calls, including transaction validation, use an ephemeral ledger storage that
reads through the shared one, is never written to disk, and is released when the
call ends. Return values are unchanged, and no runtime upgrade is required.

PR: <link to PR>
