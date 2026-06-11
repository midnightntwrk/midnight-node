#toolkit #runtime

# Fix environments configuration files and genesis state generation to prevent empty locked pool

Removes logic that assigned `MAX_SUPPLY - treasury` to the `reserve_pool` leaving `locked_pool` empty in absence of reserve config.
Now, if reserve config is absent, the reserve pool would be empty. Genesis state will likely fail in such a case, because
funding seeds would fail.

Therefore there will be `--force` flag required if any pool or treasury is empty.

All environments are now configured to mimic `mainnet` configuration in regards to Midnight Genesis reserve, locked and treasury pools.

Future chain-spec generation should not create specs with empty locked pool if some config was omitted.

'preview' Midnight genesis files are removed because they contain invalid pools.
'preview' has to be reset in order to get functionality.
Removing these files makes us sure that chain-spec will be rebuild with new ones.

PR: https://github.com/midnightntwrk/midnight-node/pull/1675
Issue: https://github.com/midnightntwrk/midnight-node/issues/1674
