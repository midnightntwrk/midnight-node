#audit #toolkit
# Remove verbose println! logging from ledger helpers

Replace unconditional `println!` calls in `intent.rs`, `transaction.rs`,
and `proving.rs` with structured `log::` macros gated by `RUST_LOG`.
Remove the sensitive intent structure dump that exposed privacy-critical
transaction internals to stdout.

PR: https://github.com/midnightntwrk/midnight-node/pull/936
Ticket: https://shielded.atlassian.net/browse/PM-22084
