#!/bin/bash

MOUNTED_DIRS=(/tmp /mnt/output /out)
mkdir -p ${MOUNTED_DIRS[@]}
chown -R appuser:appuser ${MOUNTED_DIRS[@]}

function cleanup() {
    chown -R root:root ${MOUNTED_DIRS[@]}
}
trap cleanup EXIT

runuser -u appuser /midnight-node-toolkit -- "$@"