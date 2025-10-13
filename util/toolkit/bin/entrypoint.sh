#!/bin/bash
mkdir -p /tmp
chown -R appuser:appuser /tmp /mnt/output

su appuser /midnight-node-toolkit -- "$@"