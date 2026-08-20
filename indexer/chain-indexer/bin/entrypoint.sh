#!/bin/bash

trap 'rm /var/run/chain-indexer/running' EXIT
trap 'kill -SIGINT $PID' INT
trap 'kill -SIGTERM $PID' TERM

touch /var/run/chain-indexer/running
chain-indexer &
PID=$!
wait $PID
