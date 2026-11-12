#toolkit
# Remove shx from the contract build directory check

Use the shell's directory test in build-compact and remove the unused shx
dependency chain, including braces flagged by the npm security audit.
The existing build condition and failure handling are preserved.

PR: https://github.com/midnightntwrk/midnight-node/pull/2244
Issue: https://github.com/midnightntwrk/midnight-node/issues/2242
