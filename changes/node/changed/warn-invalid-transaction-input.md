#node
# Log malformed public transaction input at warning level

Transaction validation, decoding, and fee estimation now log deserialization
failures at WARN in ledger versions 8 and 9. Invalid public input is expected
and should not trigger error-level operational alerts. Block application and
stored-state deserialization retain ERROR logging; returned errors are unchanged.

Issue: https://github.com/midnightntwrk/midnight-node/issues/2242
