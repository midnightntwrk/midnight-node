#!/usr/bin/env bash
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

# A drop-in midnight-node-toolkit running TOOLKIT_IMAGE. TOOLKIT_MOUNTS and the proving
# parameters are mounted at their host paths, so host paths work as arguments. No TTY:
# callers parse stdout. --init forwards signals to the toolkit, which would ignore them as
# PID 1; the name and label let the watchdog and an interrupted run stop the container.
set -e
IMAGE="${TOOLKIT_IMAGE:?TOOLKIT_IMAGE not set}"
mounts=()
envs=(-e "RESTORE_OWNER=$(id -u):$(id -g)")
for d in ${TOOLKIT_MOUNTS:-}; do mkdir -p "$d"; mounts+=(-v "$d:$d"); done
if [ -n "${MIDNIGHT_PP:-}" ]; then
    mkdir -p "$MIDNIGHT_PP"; mounts+=(-v "$MIDNIGHT_PP:$MIDNIGHT_PP"); envs+=(-e "MIDNIGHT_PP=$MIDNIGHT_PP")
fi
for v in ${TOOLKIT_ENV_VARS:-MN_REPLAY_CONCURRENCY MN_BEST_BLOCK_TIMEOUT_SECS}; do
    [ -n "${!v:-}" ] && envs+=(-e "$v=${!v}")
done
exec docker run --rm --init --network host \
    --name "${TOOLKIT_CONTAINER_NAME:-hf-tk-${HF_TK_OWNER:-0}-$$-$RANDOM}" --label "hf-tk-owner=${HF_TK_OWNER:-0}" \
    "${envs[@]}" "${mounts[@]}" -w "${TOOLKIT_WORKDIR:-/}" "$IMAGE" "$@"
