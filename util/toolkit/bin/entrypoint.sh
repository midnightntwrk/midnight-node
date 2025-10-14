#!/bin/bash
mkdir -p /tmp /mnt/output /out
chown -R appuser:appuser /tmp /mnt/output /out

runuser -u appuser /midnight-node-toolkit -- "$@"