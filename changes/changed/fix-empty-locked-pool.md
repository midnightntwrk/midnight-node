#toolkit #runtime

# Fix environments configuration files and genesis state generation process to prevent empty locked pool

**Makes all /res Reserve and ICS configs valid (non zero values). All genesis files and chain-specs, with known exception for Preview, have non empty locked pool amount.**

Removes logic that assigned `MAX_SUPPLY - treasury` to the `reserve_pool` leaving `locked_pool` empty in absence of reserve config.
Now, if reserve config is absent, the reserve pool would be empty. Genesis state will likely fail in such a case, because
funding seeds would fail.

Therefore there will be `--allow-empty-pools` flag required if any pool or treasury is empty.
Future chain-spec generation should not create specs with empty locked pool if some config was omitted.

All environments are now configured to mimic `mainnet` amounts configuration in regards to Midnight Genesis reserve, locked and treasury pools.

Durable environments genesis states and chain-specs are not re-created. We keep chain-spec as on environment and genesis-state consistent with chain-specs. This is way CI likes.

Currently there is discrepancy between pool amounts in config for Preview environment only. Preview needs reset and new chain-spec will be correct if generated from current config files.

PR: https://github.com/midnightntwrk/midnight-node/pull/1675
Issue: https://github.com/midnightntwrk/midnight-node/issues/1674
