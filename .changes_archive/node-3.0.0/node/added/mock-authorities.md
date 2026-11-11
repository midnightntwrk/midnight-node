#node
# Allow test authorities with real Cardano observations

Extend USE_MAIN_CHAIN_FOLLOWER_MOCK with an opt-in authorities value that
reuses existing validator and governance mocks while reading Cardano
observations from db-sync. Existing true and false values retain their
behavior and boolean serialization; the default remains false.

PR: https://github.com/midnightntwrk/midnight-node/pull/2290
