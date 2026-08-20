#!/bin/bash

trap 'rm /var/run/wallet-indexer/running' EXIT
trap 'kill -SIGINT $PID' INT
trap 'kill -SIGTERM $PID' TERM

touch /var/run/wallet-indexer/running
wallet-indexer &
PID=$!
wait $PID
