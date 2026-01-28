#toolkit
# Improve toolkit error logging when sending transactions

- Fix issue where toolkit would fail to flush logs on exit, causing logs to get lost
- Log more events when sending a transaction
- Return exit code non-zero when a send failure occurs


