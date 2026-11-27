#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Group 10 on a network, validators dropping out: one validator pod is killed while the
# others carry on; its controller recreates it and it must catch up. Opt-in: it needs
# KUBE_CONTEXT and DROPOUT_POD in the network settings.
#
#   dropout.sh <when>     a label for the moment, e.g. wave2, before-upgrade, after-fork
#
# On local-env the rollout does this itself (local/rollout.sh).
set -e
export TARGET=network
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

WHEN="${1:?usage: dropout.sh <when>}"
evidence_dir "g10_dropout_$WHEN"
[ -n "${KUBE_CONTEXT:-}" ] && [ -n "${DROPOUT_POD:-}" ] || cannot_run "set KUBE_CONTEXT and DROPOUT_POD, the validator pod to kill, in the network settings"
_kubectl --request-timeout=15s get pod "$DROPOUT_POD" > /dev/null 2>&1 || cannot_run "there is no pod $DROPOUT_POD in namespace ${KUBE_NAMESPACE:-$NETWORK_NAME}"
CHECK_NAME_PREFIX="$WHEN-"
SECONDS_TO_BE_READY=600
SECONDS_TO_CATCH_UP=900
POD_UID_BEFORE=""; POD_IMAGE_BEFORE=""

group_start G10 "Failure and recovery drills on $NETWORK_NAME: validator pod $DROPOUT_POD killed ($WHEN)"

pod_field() { _kubectl --request-timeout=15s get pod "$DROPOUT_POD" -o jsonpath="$1" 2> /dev/null; }
pod_is_ready() { [ "$(pod_field '{.status.conditions[?(@.type=="Ready")].status}')" = True ]; }

# From the node's own log line: "... best: #123 (0x...), finalized #120 (0x...)".
pod_heights() {  # -> "<best> <finalized>"
    _kubectl --request-timeout=30s logs "$DROPOUT_POD" --since=2m 2> /dev/null | grep -aoE 'best: #[0-9]+.*finalized #[0-9]+' | tail -1 \
        | sed -E 's/best: #([0-9]+).*finalized #([0-9]+)/\1 \2/'
}

check_validator_killed() {
    local heights health
    pod_is_ready || { fail "$DROPOUT_POD is not Ready before the drill"; return; }
    heights=$(pod_heights)
    [ -n "$heights" ] || { fail "the pod's log has no 'best: #n ... finalized #n' line, so its sync cannot be followed"; return; }
    POD_UID_BEFORE=$(pod_field '{.metadata.uid}'); POD_IMAGE_BEFORE=$(pod_field '{.spec.containers[0].image}')
    _kubectl --request-timeout=30s delete pod "$DROPOUT_POD" --grace-period=0 --force --wait=false > "$EV/delete.log" 2>&1 \
        || { POD_UID_BEFORE=""; fail "kubectl delete: $(last_line_of "$EV/delete.log")"; return; }
    if health=$(chain_health 60); then pass "$DROPOUT_POD (at #${heights% *}, $POD_IMAGE_BEFORE) was killed; the rest of the network: $health"
    else fail "with $DROPOUT_POD down: $health"; fi
}

check_killed_validator_rejoins() {
    [ -n "$POD_UID_BEFORE" ] || { skip "nothing was killed"; return; }
    local waited=0 uid heights tip finalized problems
    while [ "$waited" -lt "$SECONDS_TO_BE_READY" ]; do
        uid=$(pod_field '{.metadata.uid}')
        [ -n "$uid" ] && [ "$uid" != "$POD_UID_BEFORE" ] && pod_is_ready && break
        sleep 10; waited=$(( waited + 10 ))
    done
    [ -n "$uid" ] && [ "$uid" != "$POD_UID_BEFORE" ] && pod_is_ready || { fail "no Ready replacement pod after ${SECONDS_TO_BE_READY}s"; return; }
    [ "$(pod_field '{.spec.containers[0].image}')" = "$POD_IMAGE_BEFORE" ] || { fail "recreated on $(pod_field '{.spec.containers[0].image}'), it ran $POD_IMAGE_BEFORE"; return; }
    waited=0
    while [ "$waited" -lt "$SECONDS_TO_CATCH_UP" ]; do
        heights=$(pod_heights); tip=$(node_height); finalized=$(finalized_height)
        [ -n "$heights" ] && [ "${heights% *}" -ge $(( tip - 5 )) ] && [ "${heights#* }" -ge $(( finalized - 5 )) ] && break
        sleep 15; waited=$(( waited + 15 ))
    done
    _kubectl --request-timeout=30s logs "$DROPOUT_POD" --since=30m > "$EV/pod.log" 2>&1 || true
    [ -n "$heights" ] && [ "${heights% *}" -ge $(( tip - 5 )) ] && [ "${heights#* }" -ge $(( finalized - 5 )) ] \
        || { fail "the pod is at ${heights:-no height logged} (best, finalized); the network at #$tip, finalized #$finalized, after ${SECONDS_TO_CATCH_UP}s ($EV/pod.log)"; return; }
    problems=$(grep -aiE 'panicked|state root mismatch|bad block|block import failed|database corrupt' "$EV/pod.log" | head -3 | cut -c1-160 | tr '\n' ' ')
    [ -z "$problems" ] || { fail "it caught up, but its log says: $problems"; return; }
    pass "recreated on the same image, caught up to best #${heights% *}, finalized #${heights#* }; no panic, bad block or corruption in its log"
}

bullet "Validators dropping out: the others carry on and it catches up"
check validator-killed         "a validator pod is killed: the rest of the network keeps producing and finalizing" check_validator_killed
check killed-validator-rejoins "its controller recreates it on the same image and it catches up"                   check_killed_validator_rejoins

group_finish
