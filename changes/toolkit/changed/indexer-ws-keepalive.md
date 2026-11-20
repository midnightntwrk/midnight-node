#toolkit #indexer
# Keep indexer subscriptions alive behind proxies

Indexer subscriptions now send a WebSocket ping every 20 seconds while they wait for the server.
The indexer slows its progress heartbeat to as little as one message every 4 minutes once a
wallet's progress stops changing, and a proxy in front of it, such as an AWS load balancer with
its default 60-second idle timeout, closed the quiet connection. `--indexer-url` wallet syncs then
failed with `WebSocket protocol error: Connection reset without closing handshake`.

PR: https://github.com/midnightntwrk/midnight-node/pull/PRNUM
