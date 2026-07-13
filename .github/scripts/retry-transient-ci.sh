#!/usr/bin/env bash
# Run "$@" up to 3 times, retrying ONLY on a known-transient CI failure:
#
#   * shared self-hosted buildkitd recycled out from under the build — a
#     concurrent job connected with a different earthly settings hash and
#     SIGTERMed the daemon, taking down every in-flight build on the host;
#   * a dropped ghcr.io layer push ("timeout awaiting response headers" and
#     friends) — buildkit's registry response-header timeout isn't tunable.
#
# Earthly's cache makes the retry cheap (work that already succeeded is
# skipped). A genuine build/test failure does NOT match TRANSIENT_RE and fails
# fast on attempt 1, so a real regression is never masked by a blind retry.
#
# Only "$1" (the command name) is echoed — never "$*", which would leak
# --secret values passed as arguments into the CI log.
set -uo pipefail

TRANSIENT_RE='Shutdown signal received|killing buildkit pid|transport is closing|rpc error: code = Unavailable|connection reset by peer|failed to connect to buildkit|timeout awaiting response headers|TLS handshake timeout|i/o timeout'
LOG="${RUNNER_TEMP:-/tmp}/retry-transient-ci.$$.log"

for attempt in 1 2 3; do
  echo "----- $1: attempt $attempt/3 -----"
  "$@" 2>&1 | tee "$LOG"
  rc=${PIPESTATUS[0]}
  [ "$rc" -eq 0 ] && exit 0
  if [ "$attempt" -lt 3 ] && grep -qE "$TRANSIENT_RE" "$LOG"; then
    echo "::warning::$1: transient CI failure (buildkit recycle / registry push) — retrying in 30s (attempt $attempt/3)"
    sleep 30
    continue
  fi
  echo "$1 failed (rc=$rc); not a transient signature or attempts exhausted — not retrying"
  exit "$rc"
done
