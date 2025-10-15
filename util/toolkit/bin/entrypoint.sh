#!/bin/bash

MOUNTED_DIRS=(/tmp /mnt/output /out)
mkdir -p ${MOUNTED_DIRS[@]}
chown -R appuser:appuser ${MOUNTED_DIRS[@]}

function cleanup() {
    chown -R 1000:1000 ${MOUNTED_DIRS[@]}
}
trap cleanup EXIT

runuser -u appuser /midnight-node-toolkit -- "$@"