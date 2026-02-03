#toolkit
# Fix multiple `--dest-url` not respecting `--rate` correctly

When using multiple `--dest-url` arguments, the toolkit would send to each destination at `--rate`.

It should instead send the txs at `--rate`, but each tx sent should go to a different `--dest-url` (cycling through the list)

PR: https://github.com/midnightntwrk/midnight-node/pull/472
Ticket: https://shielded.atlassian.net/browse/PM-21231
