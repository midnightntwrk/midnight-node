#!/bin/bash

trap 'rm /var/run/indexer-api/running' EXIT
trap 'kill -SIGINT $PID' INT
trap 'kill -SIGTERM $PID' TERM

touch /var/run/indexer-api/running
indexer-api &
PID=$!
wait $PID
