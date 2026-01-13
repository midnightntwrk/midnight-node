#!/bin/bash

MOUNTED_DIRS=(/tmp /mnt/output /out)

# Only mount directory when MN_FETCH_CACHE uses redb: prefix (file-based caching)
if [[ "$MN_FETCH_CACHE" == redb:* ]]; then
    FETCH_CACHE_PATH="${MN_FETCH_CACHE#redb:}"
    FETCH_CACHE_DIR="$(dirname "$FETCH_CACHE_PATH")"
    MOUNTED_DIRS+=("$FETCH_CACHE_DIR")
fi

mkdir -p ${MOUNTED_DIRS[@]}
chown -R appuser:appuser ${MOUNTED_DIRS[@]}

function cleanup() {
    if [ -n "$RESTORE_OWNER" ]; then
        chown -R "$RESTORE_OWNER" ${MOUNTED_DIRS[@]}
    fi
}
trap cleanup EXIT

runuser -u appuser /midnight-node-toolkit -- "$@"
