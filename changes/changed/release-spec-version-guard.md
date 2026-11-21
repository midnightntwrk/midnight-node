# Release refuses a changed runtime at an unbumped spec_version

The release-image workflow fails when `runtime`, `pallets`, `ledger` or `metadata`
changed since the newest `runtime-*` tag but `spec_version` did not increase, so a
changed runtime ABI never ships under an existing runtime identity. It does not run
when `skip-runtime` is set.

PR: https://github.com/midnightntwrk/midnight-node/pull/2209
Issue: split out of the review of https://github.com/midnightntwrk/midnight-node/issues/1474
