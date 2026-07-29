#local-env

# Commands for governance actions of moving consensus engine state machine

Two new commands are added for testing/executing consensus engine state transitions:

- `consensus-upgrade-arm-babe` for executing the first manual state machine transition
- `consensus-upgrade-schedule-flip` for executing the second manual state machine transition

Issue: https://github.com/midnightntwrk/midnight-node/issues/1740
PR: https://github.com/midnightntwrk/midnight-node/pull/1918
