#node #cnight-observation #performance
# Chunk the cNIGHT observation cache catch-up pull

The bulk cNIGHT observation cache warms/slides its in-memory window in a background `refresh`, which pulled the whole `[from_block, target_end]` range in a single `bulk_pull` with `LARGE_LIMIT`. In steady state that range is small, but when a node is far behind tip (cold start, or after a lag) the span can be hundreds of thousands of Cardano blocks, and one query over it is a multi-minute db-sync scan that stalls block import.

The refresh now pulls the range in bounded block-span chunks (`REFRESH_CHUNK_BLOCKS`, 5000 blocks), committing `snapshot_end_block` after each chunk so an interrupted catch-up resumes from the last committed block. This is a cache-warming change only: the union of chunks covers exactly the same range, so the consensus inherent payload is unaffected (the per-block inherent path in `get_utxos_up_to_capacity` still queries through tip, unchanged).

PR: https://github.com/midnightntwrk/midnight-node/pull/1889
