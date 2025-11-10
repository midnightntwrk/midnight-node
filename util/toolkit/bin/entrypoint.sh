#!/bin/bash

MOUNTED_DIRS=(/tmp /mnt/output /out)
# Add cache directory from environment variable, default to /.cache/sync
CACHE_DIR="${MN_SYNC_CACHE:-/.cache/sync}"
MOUNTED_DIRS+=("$CACHE_DIR")

mkdir -p ${MOUNTED_DIRS[@]}
chown -R appuser:appuser ${MOUNTED_DIRS[@]}

function cleanup() {
    if [ -n "$RESTORE_OWNER" ]; then
        chown -R "$RESTORE_OWNER" ${MOUNTED_DIRS[@]}
    fi
}
trap cleanup EXIT

runuser -u appuser /midnight-node-toolkit -- "$@"