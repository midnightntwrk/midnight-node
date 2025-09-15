#!/bin/bash

trap 'rm /var/run/indexer-standalone/running' EXIT
trap 'kill -SIGINT $PID' INT
trap 'kill -SIGTERM $PID' TERM

touch /var/run/indexer-standalone/running
indexer-standalone &
PID=$!
wait $PID
