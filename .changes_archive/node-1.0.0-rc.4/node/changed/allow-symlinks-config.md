#node
# Add `unsafe_allow_symlinks` config option when loading files

The new `unsafe_allow_symlinks` config option permits the use of symlinks when loading configuration files on node boot. Disabled by default to prevent symlink attacks.

PR: https://github.com/midnightntwrk/midnight-node/pull/1372
