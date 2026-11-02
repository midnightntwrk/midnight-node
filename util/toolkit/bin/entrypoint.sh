#!/bin/bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# http://www.apache.org/licenses/LICENSE-2.0
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.


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
