#audit #toolkit
# Propagate errors from save_intents_to_file

save_intents_to_file previously reported success even when serialization or
file writing failed, silently producing incomplete intent files. Errors are now
propagated to callers, and any partially written files are cleaned up on failure.

PR: https://github.com/midnightntwrk/midnight-node/pull/873
JIRA: https://shielded.atlassian.net/browse/PM-20209
