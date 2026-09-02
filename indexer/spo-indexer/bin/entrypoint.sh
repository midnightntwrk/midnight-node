#!/bin/bash
set -euo pipefail

RUN_FILE=/var/run/spo-indexer/running
trap 'rm -f "$RUN_FILE"' EXIT
trap 'kill -SIGINT $PID' INT
trap 'kill -SIGTERM $PID' TERM

touch "$RUN_FILE"
spo-indexer &
PID=$!
wait $PID
