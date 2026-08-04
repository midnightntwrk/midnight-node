#node
# Add Midnight-specific reference hardware profile for benchmark machine checks

Replaces the upstream `SUBSTRATE_REFERENCE_HARDWARE` defaults with a
Midnight-specific profile (`node/src/midnight_reference_hardware.json`)
embedded in the binary. The profile is used in two places:

- `midnight-node benchmark machine` validates the host against it and
  reports per-metric pass/fail.
- Node startup warns authorities whose hardware does not meet the
  profile (existing check, now against Midnight's minimums rather than
  Substrate's).

A new helper script `scripts/benchmark/generate-reference-hardware.sh`
runs `benchmark machine` on the current host and rewrites the JSON,
treating that host as the new reference. Run it on dedicated reference
hardware whenever the supported floor changes.

PR: https://github.com/midnightntwrk/midnight-node/pull/1511
Issue: https://github.com/midnightntwrk/midnight-node/issues/1509
