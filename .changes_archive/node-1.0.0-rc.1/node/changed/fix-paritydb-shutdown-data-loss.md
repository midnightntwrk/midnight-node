#node
# Fix chain-state truncation after unclean shutdown

Explicitly drop the Substrate database backend after the tokio runtime shuts
down, ensuring parity-db's WAL pipeline is fully drained on SIGTERM. Without
this, leaked Arc references in aborted async tasks could prevent the Drop impl
from running, causing silent chain-state truncation on next startup.

PR: https://github.com/midnightntwrk/midnight-node/pull/1140
