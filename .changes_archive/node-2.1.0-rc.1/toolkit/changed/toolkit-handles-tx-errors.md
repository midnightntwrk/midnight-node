#toolkit
# Toolkit sender handles transaction errors with terminal-status messages.

Added new SenderError variants and reshaped wait_for_best_block to return a Result with terminal-status messages. Updated send_and_log to tag/log per failure mode.

PR: https://github.com/midnightntwrk/midnight-node/pull/1323
