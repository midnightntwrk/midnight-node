#cfg
# Add guardnet and ddosnet cfg presets

Add `res/cfg/guardnet.toml` and `res/cfg/ddosnet.toml` so the binary
recognizes `CFG_PRESET=guardnet` and `CFG_PRESET=ddosnet`. Without
these files nodes crash immediately with "Failed to load config
guardnet/ddosnet".

PR: https://github.com/midnightntwrk/midnight-node/pull/868
