#node
# Bump yamux to 0.13.8 to prevent panics

Backport of a fix from polkadot-sdk stable2509-3 (paritytech/polkadot-sdk#10479). Updates yamux from 0.13.6 to 0.13.8 to avoid panics when tungstenite websocket connections are used with yamux in the p2p networking layer.

PR: https://github.com/midnightntwrk/midnight-node/pull/755
