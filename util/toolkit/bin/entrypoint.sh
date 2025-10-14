#!/bin/bash
mkdir -p /tmp /mnt/output
chown -R appuser:appuser /tmp /mnt/output

su appuser /midnight-node-toolkit -- "$@"