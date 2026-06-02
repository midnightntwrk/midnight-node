#runtime

# Fix motion cleanup and member ordering in governance pallets

Ensure motions are removed from storage on close even when dispatch
fails. Sort authority pairs before unzipping to keep member associations
aligned.

PR: https://github.com/midnightntwrk/midnight-node/pull/803
