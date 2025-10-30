#!/bin/bash

MOUNTED_DIRS=(/tmp /mnt/output /out)
mkdir -p ${MOUNTED_DIRS[@]}
chown -R appuser:appuser ${MOUNTED_DIRS[@]}

function cleanup() {
    if [ -n "$RESTORE_OWNER" ]; then
        chown -R "$RESTORE_OWNER" ${MOUNTED_DIRS[@]}
    fi
}
trap cleanup EXIT

runuser -u appuser /midnight-node-toolkit -- "$@"