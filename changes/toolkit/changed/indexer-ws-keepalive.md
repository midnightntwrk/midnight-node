#toolkit #indexer
# Keep indexer subscriptions alive behind proxies

Indexer subscriptions now send a WebSocket ping every 20 seconds for as long as they are open.
The indexer slows its progress heartbeat to as little as one message every 4 minutes once a
wallet's progress stops changing, and a proxy in front of it, such as an AWS load balancer with
its default 60-second idle timeout, closed the quiet connection. `--indexer-url` wallet syncs then
failed with `WebSocket protocol error: Connection reset without closing handshake`.

A subscription that receives nothing for a minute, not even a reply to its pings, now fails with a
`connection lost` error. The stall timeout for the shielded and unshielded progress streams rises
from 90 seconds to 5 minutes, above the indexer's longest heartbeat gap, so a slow but live
wallet is no longer cut off early and synced with incomplete coins.

PR: https://github.com/midnightntwrk/midnight-node/pull/PRNUM
