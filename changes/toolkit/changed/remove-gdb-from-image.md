#toolkit #docker #security

# Remove gdb from the toolkit image

Drop `gdb-16.3` from `images/toolkit/Dockerfile`, matching the node image.
Reduces attack surface (no in-container ptrace-based memory inspection) and
image size; `strace`, `procps-ng`, `vim`, `jq` and `tree` remain for triage.

PR: https://github.com/midnightntwrk/midnight-node/pull/2187
