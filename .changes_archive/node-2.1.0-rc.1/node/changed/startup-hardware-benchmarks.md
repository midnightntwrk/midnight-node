#node
# Run hardware benchmarks on node startup

Run the standard Substrate hardware benchmark checks when `midnight-node` starts, print the measured scores to the logs, warn authorities when the machine is below the reference requirements, and forward the benchmark data to telemetry. Add `--no-hardware-benchmarks` to disable the startup check when needed.

PR: https://github.com/midnightntwrk/midnight-node/pull/1394/
Issue: https://github.com/midnightntwrk/midnight-node/issues/1395
