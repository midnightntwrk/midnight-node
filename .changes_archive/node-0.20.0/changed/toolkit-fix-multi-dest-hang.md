#toolkit
# Fix hang when using multiple destination URLs

Fixes an issue where the toolkit would hang indefinitely when using multiple `--dest-url` arguments. The hang occurred because tx progress subscriptions were being dropped before finalization was complete, causing the async tasks to never resolve.

PR: https://github.com/midnightntwrk/midnight-node/pull/472
