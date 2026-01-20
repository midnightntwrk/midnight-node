#toolkit
# Fix send connection timeout after long sync

Error message:
```
ERROR: failed sending to wss://preview.midnight.network: RPC error: RPC error: client error: The background task closed connection closed; restart required
```

This was fixed by ensuring the send connection is only opened just before it is used.

PR: https://github.com/midnightntwrk/midnight-node/pull/414
Ticket: https://shielded.atlassian.net/browse/PM-20962
