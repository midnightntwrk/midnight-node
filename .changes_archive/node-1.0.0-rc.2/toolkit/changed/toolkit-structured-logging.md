#toolkit
# Use tracing for structured log fields

Switched key-value log calls from `log` to `tracing` so structured fields
are emitted by `tracing-subscriber` instead of being silently dropped.

PR: https://github.com/midnightntwrk/midnight-node/pull/1230
