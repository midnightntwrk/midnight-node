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

# Table SEC: security regressions and release completeness of the matrix under test. Needs no
# toolkit replay, so it also runs after the SafeMode drill.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

EV="$EVIDENCE_DIR/security"; mkdir -p "$EV"
indexer_resolve || true
git -C "$REPO_ROOT" fetch -q origin "refs/tags/$L8_REF:refs/tags/$L8_REF" 2>/dev/null || true
if [ -n "${L9_REF:-}" ]; then
    git -C "$REPO_ROOT" fetch -q origin "refs/tags/$L9_REF:refs/tags/$L9_REF" 2>/dev/null || true
    REF_L9="$L9_REF"
else REF_L9=$(git -C "$L9_ROOT" rev-parse HEAD 2>/dev/null || echo ""); fi
REF_L8="$L8_REF"
N8="$L8_NODE_IMAGE"; T8="$L8_TOOLKIT_IMAGE"
N9="${DEPLOYED_NODE_IMAGE:-$(l9_node_image)}"; T9="$(l9_toolkit_image)"
IDX_COMMIT="${INDEXER_SOURCE_COMMIT:-}"
if [ -z "$IDX_COMMIT" ]; then [[ "${INDEXER_TAG##*-}" =~ ^[0-9a-f]{8}$ ]] && IDX_COMMIT="${INDEXER_TAG##*-}" || IDX_COMMIT="v$INDEXER_TAG"; fi

t_table SEC "Security regressions and release completeness on $NETWORK_NAME: node $REF_L8 -> ${REF_L9:-?}, indexer $INDEXER_TAG"

in_ref() { git -C "$REPO_ROOT" cat-file -e "$1^{commit}" 2>/dev/null && git -C "$REPO_ROOT" merge-base --is-ancestor "$1" "$2" 2>/dev/null; }
in_indexer() {  # <commit> -> yes | no | unknown
    local st
    st=$(curl -s -m 20 ${GITHUB_TOKEN:+-H "Authorization: Bearer $GITHUB_TOKEN"} \
        "https://api.github.com/repos/midnightntwrk/midnight-indexer/compare/$1...$IDX_COMMIT" | jq -r '.status // empty')
    case "$st" in ahead|identical) echo yes ;; "") echo unknown ;; *) echo no ;; esac
}
MATRIX="$EV/required_commits.md"
printf '| Component | Commit | Reference | Description | In build |\n|---|---|---|---|---|\n' > "$MATRIX"
c_prov() {  # <component>
    local comp=$1 c ref desc ok=0 missing="" unknown="" r files=("$SUITE_DIR/checks/required-commits.tsv")
    [ -n "${SEC_REQUIRED_COMMITS_FILE:-}" ] && files+=("$SEC_REQUIRED_COMMITS_FILE")
    while IFS=$'\t' read -r cmp c ref desc; do
        [ "$cmp" = "$comp" ] || continue
        case "$comp" in
            node-l8) in_ref "$c" "$REF_L8" && r=yes || r=no ;;
            node-l9) [ -n "$REF_L9" ] && { in_ref "$c" "$REF_L9" && r=yes || r=no; } || r=unknown ;;
            indexer) r=$(in_indexer "$c") ;;
        esac
        printf '| %s | %s | %s | %s | %s |\n' "$comp" "$c" "$ref" "$desc" "$r" >> "$MATRIX"
        case "$r" in yes) ok=$((ok + 1)) ;; no) missing="$missing $c($ref)" ;; *) unknown="$unknown $c" ;; esac
    done < <(grep -hv '^#' "${files[@]}")
    if [ "$ok" = 0 ] && [ -z "$missing$unknown" ]; then t_fail "no required commits listed for $comp"
    elif [ -n "$missing" ]; then t_fail "missing:$missing"
    elif [ -n "$unknown" ]; then t_warn "$ok present; could not verify:$unknown"
    else t_pass "$ok required commits present (evidence/security/required_commits.md)"; fi
}
t_check SEC-PROV-1 "-" "required commits are in the pre-fork node tag $REF_L8" c_prov node-l8
t_check SEC-PROV-2 "-" "required commits are in the post-fork node ref ${REF_L9:-?}" c_prov node-l9
t_check SEC-PROV-3 "-" "required commits are in the indexer image's source ($IDX_COMMIT)" c_prov indexer

pin_ledger8() { git -C "$REPO_ROOT" show "$1:Cargo.toml" 2>/dev/null | grep -m1 'mn-ledger-8 *=' | grep -o 'version = "[^"]*"' | cut -d'"' -f2; }
c_pin_ledger8() {
    local a b; a=$(pin_ledger8 "$REF_L8"); b=$(pin_ledger8 "$REF_L9")
    [ "$a" = "$L8_EXPECTED_LEDGER" ] && [ "$b" = "$L8_EXPECTED_LEDGER" ] && t_pass "both tags pin mn-ledger-8 $a" || t_fail "$REF_L8 pins '$a', $REF_L9 pins '$b'"
}
c_pin_ledger9() {
    local pin now; pin=$(git -C "$REPO_ROOT" show "$REF_L9:Cargo.lock" 2>/dev/null | grep -A2 '^name = "midnight-ledger-v9"$' | grep -o 'tag=[^#]*' | head -1 | cut -d= -f2)
    now=$(ledger_version)
    if [ "$pin" != "$L9_EXPECTED_LEDGER_TAG" ]; then t_fail "Cargo.lock pins '$pin', expected $L9_EXPECTED_LEDGER_TAG"
    elif [ -z "$now" ] || [ -z "$(spec_version)" ]; then t_fail "Cargo.lock pins $pin; the node does not report its ledger or spec"
    elif [ "$(spec_version)" -ge "$LEDGER9_SPEC_FLOOR" ] && [[ "$now" != *"$L9_EXPECTED_LEDGER_TAG"* ]]; then t_fail "Cargo.lock pins $pin, the chain reports '$now'"
    else t_pass "Cargo.lock pins $pin; chain reports '$now'"; fi
}
c_pin_psdk() {
    local l9; l9=$(git -C "$REPO_ROOT" show "$REF_L9:Cargo.toml" 2>/dev/null | grep -oE 'polkadot-stable[0-9]+|branch = "stable[0-9]+"' | sort -u | tr '\n' ' ')
    [[ "$l9" == *2606* ]] && [[ "$l9" != *2603* ]] && t_pass "$REF_L9: $l9" || t_fail "$REF_L9 pins '$l9', expected stable2606 only"
}
t_check SEC-PIN-1 "-" "both node tags pin ledger $L8_EXPECTED_LEDGER" c_pin_ledger8
t_check SEC-PIN-2 "-" "the post-fork tag pins $L9_EXPECTED_LEDGER_TAG and the chain runs it" c_pin_ledger9
t_check SEC-PIN-3 "-" "the post-fork tag builds on polkadot-sdk stable2606" c_pin_psdk

has_gdb() { docker run --rm --entrypoint sh "$1" -c 'command -v gdb >/dev/null 2>&1 && echo present || echo absent' 2>/dev/null | tail -1; }
tar_version() { docker run --rm --entrypoint sh "$1" -c 'grep -m1 "\"version\"" "$(npm root -g)/npm/node_modules/tar/package.json" 2>/dev/null | tr -dc 0-9.' 2>/dev/null | tail -1; }
c_gdb() {
    local img r out="" bad=""
    for img in "$N8" "$T8" "$N9" "$T9"; do
        r=$(has_gdb "$img"); out="$out ${img##*/}=$r"; [ "$r" = absent ] || bad="$bad ${img##*/}"
    done
    [ -z "$bad" ] && t_pass "$out" || t_fail "gdb present or unreadable in:$bad ($out)"
}
c_tar() {
    local a b; a=$(tar_version "$T8"); b=$(tar_version "$T9")
    [ -n "$a" ] && [ -n "$b" ] && ver_ge "$a" 7.5.19 && ver_ge "$b" 7.5.19 && t_pass "toolkit ${T8##*:}: tar $a; toolkit ${T9##*:}: tar $b" \
        || t_fail "tar '${a:-?}' / '${b:-?}', need >= 7.5.19 (GHSA-23hp-3jrh-7fpw)"
}
t_check SEC-IMG-1 "node-1.0.300 notes" "gdb is absent from the node and toolkit images of both releases" c_gdb
t_check SEC-IMG-2 "node-1.0.300 notes" "npm's bundled tar is >= 7.5.19 in both toolkit images" c_tar

c_epoch_span() {
    [ -n "$INDEXER_GQL" ] || { t_skip "no indexer answering"; return; }
    local big small; big=$(gql_query '{ registeredTotalsSeries(fromEpoch: 0, toEpoch: 9223372036854775807) { __typename } }' | jq -r '.errors[0].message // empty')
    small=$(gql_query '{ registeredTotalsSeries(fromEpoch: 0, toEpoch: 10) { __typename } }' | jq -e '.data.registeredTotalsSeries != null' >/dev/null 2>&1 && echo answered || echo failed)
    if [[ "$big" == *"epoch range too large"* ]] && [ "$small" = answered ]; then t_pass "i64::MAX span refused ('$big'); span 10 $small"
    elif [[ "$big" == *"Unknown field"* ]] || [[ "$big" == *"Cannot query field"* ]]; then t_skip "this indexer-api has no registeredTotalsSeries"
    else t_fail "i64::MAX span -> '${big:-no error}'; span 10 $small"; fi
}
t_check SEC-IDX-1 "indexer#1455" "indexer-api refuses an unbounded epoch range and answers a bounded one" c_epoch_span

c_sigkill() {
    [ "$TARGET" = local ] || { t_skip "local-env only"; return; }
    [ "${SEC_SIGKILL:-1}" = 1 ] || { t_skip "SEC_SIGKILL=0"; return; }
    local victim=midnight-node-4 url ref b tip fin bh
    url=$(node_url "$victim"); ref="$NODE_HTTP"
    docker kill -s KILL "$victim" >/dev/null 2>&1 || true; sleep 3; docker start "$victim" >/dev/null
    for _ in $(seq 1 30); do
        b=$(best_height_at "$url"); tip=$(best_height_at "$ref")
        [ -n "$b" ] && [ -n "$tip" ] && [ "$b" -ge $(( tip - 2 )) ] && break
        sleep 10
    done
    if docker logs "$victim" --since 6m 2>&1 | grep -qE 'root should be in the arena|IncompatibleColumnConfig|panicked'; then t_fail "arena or panic lines after the restart"; return; fi
    [ -n "$b" ] && [ "$b" -ge $(( tip - 2 )) ] || { t_fail "$victim at ${b:-unreachable}, tip $tip"; return; }
    fin=$(finalized_height_at "$ref"); bh=$(block_hash_at "$ref" "$fin")
    [ "$(state_root_at "$url" "$bh")" = "$(state_root_at "$ref" "$bh")" ] && t_pass "$victim killed, resumed to #$b (tip $tip), stateRoot agrees at #$fin" \
        || t_fail "$victim resumed but its stateRoot at #$fin differs"
}
t_check SEC-NODE-1 "-" "a validator killed with SIGKILL resumes from its own state and converges" c_sigkill

c_ps() {
    local a b; a=$(proof_server_version "$PS_L8_URL"); b=$(proof_server_version "$PS_L9_URL")
    [ -n "$a$b" ] || { t_skip "no proof server running (client_stack.sh starts them)"; return; }
    [ "$a" = "$PS_L8_TAG" ] && [ "$b" = "$PS_L9_TAG" ] && t_pass "$a / $b" || t_fail "reported '$a' / '$b', matrix $PS_L8_TAG / $PS_L9_TAG"
}
c_client_ledgers() {
    [ -d "$CLIENTS_DIR/node_modules" ] || { t_skip "clients/ not installed"; return; }
    local v l8 l9; v=$(clients_versions); l8=$(client_version ledger-v8 "$v"); l9=$(client_version ledger-v9 "$v")
    if [ "=$l8" != "$L8_EXPECTED_LEDGER" ]; then t_fail "client ledger-v8 $l8, chain $L8_EXPECTED_LEDGER"
    elif [[ "$L9_EXPECTED_LEDGER_TAG" != *"${l9#1.0.0-}"* ]]; then t_warn "client ledger-v9 $l9 while the chain runs $L9_EXPECTED_LEDGER_TAG"
    else t_pass "client ledger-v8 $l8, ledger-v9 $l9"; fi
}
t_check SEC-CLI-1 "-" "proof servers report the matrix versions" c_ps
t_check SEC-CLI-2 "-" "the wallet SDK and Midnight.js bundle the chain's ledger builds" c_client_ledgers

t_finish
