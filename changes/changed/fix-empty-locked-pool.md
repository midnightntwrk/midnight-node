#toolkit #runtime

# Fix environments configuration files and genesis state generation to prevent empty locked pool

Removes logic that assigned `MAX_SUPPLY - treasury` to the `reserve_pool` leaving `locked_pool` empty in absence of reserve config.
Now, if reserve config is absent, the reserve pool would be empty. Genesis state will likely fail in such a case, because
funding seeds would fail.

All environments are now configured to mimic `mainnet` configuration in regards to Midnight Genesis reserve, locked and treasury pools.

Additionally warning is printed when genesis state generation creates a empty reserve or treasury pools.

Future chain-spec generation should not create specs with empty locked pool if some config was omitted.

PR: https://github.com/midnightntwrk/midnight-node/pull/1675
Issue: https://github.com/midnightntwrk/midnight-node/issues/1674
