#node
# Allow test authorities with real Cardano observations

Add an opt-in USE_MAIN_CHAIN_FOLLOWER_MOCK_AUTHORITIES setting that reuses the
existing validator and governance mocks while reading Cardano observations
from db-sync. The setting defaults to false; USE_MAIN_CHAIN_FOLLOWER_MOCK
retains its existing full-mock behavior.

PR: https://github.com/midnightntwrk/midnight-node/pull/2290
