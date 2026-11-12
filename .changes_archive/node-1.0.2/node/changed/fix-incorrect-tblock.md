# Fix incorrect tblock leading to inconsistent transaction validation

A warm transaction validation cache leads to different transaction validation results. This change allows the node to accept inconsistent transactions, and fixes a datetime where future inconsistent transactions will not be accepted.

Config values added; with defaults;
```toml
# 12 seconds == 2 slots
tblock_correction_offset = 12
# Tuesday, August 4, 2026 at 12:00:00 AM (UTC)
tblock_correction_disable_after = 1785801600
```

PR: https://github.com/midnightntwrk/midnight-node/pull/1932
Issue: https://github.com/midnightntwrk/midnight-node/issues/1924
