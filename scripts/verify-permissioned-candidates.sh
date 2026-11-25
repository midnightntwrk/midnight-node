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

# Checks a new permissioned-candidates list before it goes to `change-federated-ops`, and
# optionally compares it with what the chain observed. Each ERROR says what the mistake breaks.
# `change-federated-ops` checks only that keys are hex, and a 2.1 runtime accepts a wrong-length
# BABE key: without this check such mistakes surface only at the v3 runtime upgrade.
# See docs/aura-to-babe-migration-runbook.md, Phase 0. Needs bash, jq and curl. Exits 1 on any ERROR.
#
# Usage: verify-permissioned-candidates.sh <new> [--current <current>] [--rpc <url> [--epoch <n>]]
#   <new>            the list for `change-federated-ops`: its PERMISSIONED_CANDIDATES text (an .env file works),
#                    or JSON: res/<network>/permissioned-candidates-config.json format, or a bare array of candidates
#   --current        the list active today, in either format: a key may not move to another candidate (DuplicatedKey)
#   --rpc            Midnight node HTTP RPC (e.g. http://localhost:9944): compare with systemParameters_getAriadneParameters
#   --epoch          Cardano epoch for --rpc (default: the node's current one)
#
# It cannot tell whether a well-formed key is the one the node loaded: check the node itself (runbook 0.1).
set -euo pipefail

usage() { sed -n '23,28p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
[ $# -ge 1 ] || usage

# Prints the candidates as a JSON array. Non-JSON input is read the way the contracts CLI reads
# PERMISSIONED_CANDIDATES: [ { key:value, … }, … ], hex without 0x.
load() {
  if jq -e . "$1" >/dev/null 2>&1; then
    jq -c 'if type == "array" then . else .initial_permissioned_candidates end' "$1"
  else
    jq -R -s -c '
      (if test("PERMISSIONED_CANDIDATES=") then sub("^[\\s\\S]*?PERMISSIONED_CANDIDATES="; "") else . end)
      | ((capture("^[^\\[]*\\[(?<b>[^\\]]*)\\]") // error("no [ … ] list found")).b)
      | [scan("\\{[^{}]*\\}") | .[1:-1]
         | [splits("[,\n]") | gsub("^\\s+|\\s+$"; "") | select(test(":"))
            | index(":") as $i | {key: (.[0:$i] | gsub("\\s"; "")), value: (.[$i + 1:] | gsub("^\\s+|\\s+$"; ""))}]
         | from_entries]
      | if length == 0 then error("no { … } candidates found") else . end' "$1"
  fi
}

LIST=$(load "$1"); shift
CURRENT=null RPC= EPOCH=
while [ $# -gt 0 ]; do
  case $1 in
    --current) CURRENT=$(load "$2"); shift 2 ;;
    --rpc) RPC=$2; shift 2 ;;
    --epoch) EPOCH=$2; shift 2 ;;
    *) usage ;;
  esac
done

rpc() {
  curl -sf --max-time 10 -H 'Content-Type: application/json' \
    -d "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"$1\",\"params\":$2}" "$RPC" | jq -e .result
}

OBSERVED=null SPEC=null
if [ -n "$RPC" ]; then
  [ -n "$EPOCH" ] || EPOCH=$(rpc sidechain_getStatus '[]' | jq .mainchain.epoch)
  OBSERVED=$(rpc systemParameters_getAriadneParameters "[$EPOCH]" | jq .permissionedCandidates)
  SPEC=$(rpc state_getRuntimeVersion '[]' | jq .specVersion)
  echo "chain: $RPC, Cardano epoch $EPOCH, runtime spec $SPEC"
fi

jq -r -n --argjson list "$LIST" --argjson current "$CURRENT" --argjson observed "$OBSERVED" \
  --argjson spec "$SPEC" '
def norm: if type == "string" then ascii_downcase | ltrimstr("0x") else null end;
def short: if type == "string" and length > 0 then "0x" + .[0:8] + "…" else "?" end;
def specs: [
  {f: "sidechain_pub_key", t: "sidechain", n: 33, ecdsa: true},
  {f: "aura_pub_key", t: "aura", n: 32},
  {f: "grandpa_pub_key", t: "grandpa", n: 32},
  {f: "babe_pub_key", t: "babe", n: 32},
  {f: "beefy_pub_key", t: "beefy", n: 33, ecdsa: true}];
def cands: (if type == "array" then . else .initial_permissioned_candidates end) | to_entries
  | map(.value as $c | {i: (.key + 1), sc: ($c.sidechain_pub_key | norm)}
      + ([specs[] | {key: .t, value: ($c[.f] | norm)}] | from_entries));
def who: "#\(.i) \(.sc | short)";
def out($lvl; $who; $msg): "\($lvl)\t\($who)\t\($msg)";
def hint($s; $v):
  ($v | length / 2) as $n
  | if $s.ecdsa | not then
      if $n == 64 then " (a secret key or a signature, not a public key?)"
      elif $n == 33 and ($v | test("^0[23]")) then " (an ECDSA key: cross-chain or BEEFY?)"
      elif $n < 32 then " (cut short when copying?)" else "" end
    elif $n == 32 then " (an sr25519/ed25519 key?)"
    elif $n == 65 then " (uncompressed: send the 33-byte compressed form)" else "" end;

($list | cands) as $new
| (if $current == null then [] else $current | cands end) as $old
| " — the v3 runtime drops the candidate from the committee (no fallback to its AURA key)" as $dropped

# 1. Every key present, hex, the right length and type.
| ($new[] | . as $c | specs[] | . as $s | $c[$s.t] as $v
   | if $v == null or $v == "" then out("ERROR"; $c | who; "\($s.f) missing" + $dropped)
     elif ($v | test("^[0-9a-f]+$") | not) or ($v | length) % 2 == 1 then out("ERROR"; $c | who; "\($s.f) is not hex")
     elif ($v | length) != $s.n * 2 then out("ERROR"; $c | who; "\($s.f) is \($v | length / 2) bytes, expected \($s.n)" + hint($s; $v) + $dropped)
     elif $s.ecdsa and ($v | test("^0[23]") | not) then out("ERROR"; $c | who; "\($s.f) is not a compressed ECDSA key (must start with 02 or 03)" + $dropped)
     else empty end),

# 2. Keys mixed up within one candidate.
  ($new[] | select(.babe != null)
   | if .babe == .grandpa then out("ERROR"; who; "babe_pub_key is its own GRANDPA (ed25519) key: keeps its seat but cannot author after the flip")
     elif .babe == .aura then out("ERROR"; who; "babe_pub_key reuses its own AURA key: BABE needs a fresh key (runbook 0.1)")
     else empty end),
  ($new[] | select(.aura != null and .aura == .grandpa) | out("ERROR"; who; "aura_pub_key equals grandpa_pub_key")),

# 3. One key owned by two candidates (in the new list, or across new and current): DuplicatedKey stops rotation.
  ($new | group_by(.sc)[] | select(length > 1 and .[0].sc != null)
   | out("ERROR"; map(who) | join(", "); "same sidechain_pub_key on \(length) candidates")),
  ([($new[] | {src: "", c: .}), ($old[] | {src: "current ", c: .})]
   | map(.src as $src | .c as $c | ["aura", "grandpa", "babe", "beefy", "sidechain"][]
         | select($c[.] != null and $c[.] != "")
         | {v: $c[.], t: ., owner: $c.sc, src: $src, who: ($src + ($c | who))})
   | group_by(.v)[]
   | select((map(.owner) | unique | length) > 1 and any(.src == ""))
   | out("ERROR"; (first(.[] | select(.src == "")) | .who);
       "same key used as " + (map("\(.t) of \(.who)") | unique | join(", "))
       + " — DuplicatedKey: from the first rotation after the runtime upgrade the committee stops rotating until the list is fixed"
       + (if ([.[] | select(.t == "babe") | .owner] as $b | any(.[]; .t == "aura" and (.owner as $o | $b | any(. != $o)))) then " (the runtime upgrade copies every AURA key into the BABE slot)" else "" end))),

# 4. --rpc: the list the chain observed matches, and every entry is valid (runbook 0.3).
  (if $observed == null then empty else
     ($observed | map({sc: (.sidechainPublicKey | norm), aura: (.keys.aura | norm), grandpa: (.keys.gran | norm),
                       babe: (.keys.babe | norm), beefy: (.keys.beef | norm), isValid, invalidReasons})) as $obs
     | " — the list was changed or lost on the way to Cardano; explain before Phase 1" as $lost
     | ($new[] | . as $c | ($obs | map(select(.sc == $c.sc)) | first) as $o
        | if $o == null then out("ERROR"; $c | who; "not in the list the chain observed" + $lost)
          else (["aura", "grandpa", "babe", "beefy"][] | select($o[.] != $c[.])
                | out("ERROR"; $c | who; "\(.) on chain is \($o[.] | short), list has \($c[.] | short)" + $lost)),
               (if $o.isValid then empty else out("ERROR"; $c | who; "chain marks it invalid: \($o.invalidReasons)") end)
          end),
       ($obs[] | select(.sc as $sc | $new | all(.sc != $sc)) | out("ERROR"; "chain"; "\(.sc | short) is on chain but not in the list" + $lost)),
       (if $spec < 3000000 then out("INFO"; "chain"; "runtime spec \($spec): isValid does not check BABE keys yet; rely on checks 1–3") else empty end)
   end)
' | awk -F'\t' -v n="$(jq length <<<"$LIST")" '
  { printf "%-5s  %-22s  %s\n", $1, $2, $3; c[$1]++ }
  END { printf "%d candidates: %d error(s)\n", n, c["ERROR"]; exit c["ERROR"] > 0 }'
