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

# Verifies that the node refuses to start when chain_spec.properties.genesis_extrinsics
# contains malformed entries. Guards against regressions of the silent-truncation bug
# (Least Authority audit finding "Issue Y", PM-19900).
#
# Usage:
#     MIDNIGHT_NODE_IMAGE=ghcr.io/midnight-ntwrk/midnight-node:<tag> \
#         tests/chainspec-validation/run.sh
#
# Exits 0 if every case meets its expectation, non-zero otherwise.

set -uo pipefail

if [ -z "${MIDNIGHT_NODE_IMAGE:-}" ]; then
    echo "MIDNIGHT_NODE_IMAGE is not set (e.g. ghcr.io/midnight-ntwrk/midnight-node:<tag>)" >&2
    exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK_DIR="$(mktemp -d -t midnight-chainspec-XXXXXX)"
SPECS_DIR="$WORK_DIR/specs"
LOG_DIR="$WORK_DIR/logs"
mkdir -p "$SPECS_DIR" "$LOG_DIR"

cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

NODE_KILL_TIMEOUT="${NODE_KILL_TIMEOUT:-30}"
NODE_BOOT_TIMEOUT="${NODE_BOOT_TIMEOUT:-60}"
PARSE_ERROR_RE='Service\(Other\("(extrinsic not a string|error decoding extrinsic as hex)'
BOOT_OK_RE='(🏆 Imported #|💤 Idle)'

echo "==> Generating baseline + malformed chainspecs in $SPECS_DIR"
docker run --rm \
    -v "$SCRIPT_DIR/generate-specs.sh:/generate-specs.sh:ro" \
    -v "$SPECS_DIR:/specs" \
    -e OUT_DIR=/specs \
    --entrypoint /bin/bash \
    "$MIDNIGHT_NODE_IMAGE" /generate-specs.sh

# Run the node image against $variant; expect it to exit non-zero and print a
# stderr line matching $expected_re. Returns 0 on pass, 1 on fail.
run_case() {
    local name="$1" variant="$2" expected_re="$3"
    local log="$LOG_DIR/$name.log"
    local cid
    cid=$(docker run -d \
        -e CFG_PRESET=dev \
        -v "$SPECS_DIR:/specs:ro" \
        "$MIDNIGHT_NODE_IMAGE" \
        --chain=/specs/"$variant" \
        --base-path=/tmp/mn-test) || return 1

    local exited=0
    for _ in $(seq 1 "$NODE_KILL_TIMEOUT"); do
        if [ "$(docker inspect -f '{{.State.Running}}' "$cid" 2>/dev/null)" != "true" ]; then
            exited=1
            break
        fi
        sleep 1
    done

    docker logs "$cid" > "$log" 2>&1 || true

    if [ "$exited" -ne 1 ]; then
        echo "  TIMEOUT — container still running after ${NODE_KILL_TIMEOUT}s (no parse error surfaced)"
        docker kill "$cid" >/dev/null 2>&1 || true
        docker rm -f "$cid" >/dev/null 2>&1 || true
        return 1
    fi

    local exit_code
    exit_code=$(docker inspect -f '{{.State.ExitCode}}' "$cid" 2>/dev/null || echo "?")
    docker rm -f "$cid" >/dev/null 2>&1 || true

    if [ "$exit_code" = "0" ]; then
        echo "  FAIL — exit 0 (expected non-zero)"
        return 1
    fi

    if ! grep -Eq "$expected_re" "$log"; then
        echo "  FAIL — exit $exit_code but stderr did not match: $expected_re"
        echo "  Last 5 log lines:"
        tail -5 "$log" | sed 's/^/    /'
        return 1
    fi

    local match
    match=$(grep -Eo "$PARSE_ERROR_RE[^\"]*" "$log" | head -1 || true)
    echo "  PASS — exit $exit_code; matched: $match"
    return 0
}

# Run the node image against the baseline spec as a dev validator; expect it to
# stay up and log a healthy marker within NODE_BOOT_TIMEOUT seconds, with no
# parse-error stderr. Returns 0 on pass, 1 on fail.
run_baseline_case() {
    local name="baseline" variant="baseline.json"
    local log="$LOG_DIR/$name.log"
    local cid
    cid=$(docker run -d \
        -e CFG_PRESET=dev \
        -v "$SPECS_DIR:/specs:ro" \
        "$MIDNIGHT_NODE_IMAGE" \
        --chain=/specs/"$variant" \
        --base-path=/tmp/mn-test \
        --validator --alice \
        --node-key=0000000000000000000000000000000000000000000000000000000000000001) || return 1

    local outcome="timeout"
    for _ in $(seq 1 "$NODE_BOOT_TIMEOUT"); do
        if [ "$(docker inspect -f '{{.State.Running}}' "$cid" 2>/dev/null)" != "true" ]; then
            outcome="exited"
            break
        fi
        docker logs "$cid" > "$log" 2>&1 || true
        if grep -Eq "$BOOT_OK_RE" "$log"; then
            outcome="booted"
            break
        fi
        sleep 1
    done

    docker logs "$cid" > "$log" 2>&1 || true
    docker kill "$cid" >/dev/null 2>&1 || true
    docker rm -f "$cid" >/dev/null 2>&1 || true

    if grep -Eq "$PARSE_ERROR_RE" "$log"; then
        echo "  FAIL — baseline spec triggered a parse error (regression in valid-input path)"
        grep -Eo "$PARSE_ERROR_RE[^\"]*" "$log" | head -1 | sed 's/^/    /'
        return 1
    fi

    case "$outcome" in
        booted)
            local match
            match=$(grep -Eo "$BOOT_OK_RE.{0,60}" "$log" | head -1 || true)
            echo "  PASS — boot marker seen: $match"
            return 0
            ;;
        exited)
            echo "  FAIL — node exited before reaching a boot marker"
            echo "  Last 5 log lines:"
            tail -5 "$log" | sed 's/^/    /'
            return 1
            ;;
        timeout)
            echo "  FAIL — no boot marker after ${NODE_BOOT_TIMEOUT}s"
            echo "  Last 5 log lines:"
            tail -5 "$log" | sed 's/^/    /'
            return 1
            ;;
    esac
}

declare -i failures=0

echo
echo "==> Case: nonstring (non-string element in middle position)"
run_case nonstring nonstring.json 'extrinsic not a string: Number\(123\)' || failures=$((failures+1))

echo
echo "==> Case: bad-hex (string, but invalid hex)"
run_case bad-hex bad-hex.json 'error decoding extrinsic as hex: \\"xxxabc\\"' || failures=$((failures+1))

echo
echo "==> Case: audit-example (audit report's exact example)"
run_case audit-example audit-example.json 'extrinsic not a string: Number\(123\)' || failures=$((failures+1))

echo
echo "==> Case: baseline (unmodified spec — node should boot cleanly)"
run_baseline_case || failures=$((failures+1))

echo
if [ "$failures" -ne 0 ]; then
    echo "==> $failures case(s) failed. Logs preserved under $WORK_DIR (will be deleted on exit; copy if you need them)."
    exit 1
fi

echo "==> All cases passed."
