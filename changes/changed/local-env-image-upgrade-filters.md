# Image upgrades honour --include/--exclude

`image-upgrade` and `full-upgrade` apply their `--include` / `--exclude` patterns to
the discovered compose services before rolling them out, followed by the
mock-validator exclusion. A filter set that matches no service fails with an error
naming the filters.

PR: https://github.com/midnightntwrk/midnight-node/pull/2208
Issue: split out of the review of https://github.com/midnightntwrk/midnight-node/issues/1474
