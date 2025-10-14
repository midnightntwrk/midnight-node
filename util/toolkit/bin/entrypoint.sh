#!/bin/bash
mkdir -p /tmp /mnt/output
chown -R appuser:appuser /tmp /mnt/output

runuser -u appuser /midnight-node-toolkit "$@"