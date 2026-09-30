#local-env

# Command for the governance action of moving the consensus engine state machine

A new command is added for testing/executing the consensus engine state transition:

- `consensus-upgrade-schedule-flip` for executing the manual state machine transition
  (`Aura` → `ScheduledFlip`) via a federated-authority motion

Issue: https://github.com/midnightntwrk/midnight-node/issues/1740
PR: https://github.com/midnightntwrk/midnight-node/pull/1918
