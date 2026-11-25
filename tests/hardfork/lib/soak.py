#!/usr/bin/env python3
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
"""Verdicts on the output of `wallet_check.mjs soak`, the crosser's log.

    soak.py <soak.json> <fork height> <fork block time, unix ms> <new spec floor> <old ledger> <new ledger> <check>

Prints "<PASS|FAIL|WARN> <detail>". Checks: crossed, prefork, inflight, recovered, history.
"""
import json
import sys
from collections import Counter
from datetime import datetime


def ms(iso):
    return int(datetime.fromisoformat(iso.replace("Z", "+00:00")).timestamp() * 1000)


def describe(a):
    err = a.get("error") or {}
    code = f" custom error {err['customError']}" if "customError" in err else ""
    why = f" ({err.get('name', '')}{code}: {err.get('message', '')[:80]})" if err else ""
    return f"#{a['i']} {a['outcome']}{why}"


def main():
    path, fork_height, fork_ms, floor = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
    old, new, check = sys.argv[5], sys.argv[6], sys.argv[7]
    soak = json.load(open(path))
    attempts = soak.get("attempts", [])
    is_new = lambda a: a.get("builtFor") is not None and a["builtFor"] >= floor
    before = [a for a in attempts if ms(a["endedAt"]) < fork_ms]
    across = [a for a in attempts if ms(a["startedAt"]) < fork_ms <= ms(a["endedAt"])]
    after = [a for a in attempts if ms(a["startedAt"]) >= fork_ms]

    if check == "crossed":
        phases = soak.get("phases", [])
        keys = " -> ".join(p["key"] for p in phases)
        settled = [p for p in phases if p["tag"] == "Settled"]
        first = int(settled[0]["key"].split()[1]) if settled else None
        last = int(settled[-1]["key"].split()[1]) if settled else None
        crossing = any(p["tag"] == "Crossing" for p in phases)
        if first is not None and first < floor and last is not None and last >= floor and soak.get("forkSeenAt"):
            print(f"PASS {len(attempts)} attempts by one process; {keys}{'' if crossing else ' (no Crossing phase observed between)'}")
        else:
            print(f"FAIL the wallet did not move from ledger {old} to ledger {new} in one run: {keys or 'no phases recorded'}")

    elif check == "prefork":
        if not before:
            print("FAIL no transfer finished before the fork block: the load started too late")
            return
        bad = [a for a in before if a["outcome"] != "landed"]
        tally = ", ".join(f"{n} {o}" for o, n in Counter(a["outcome"] for a in before).items())
        print(f"PASS {len(before)} transfers before #{fork_height}: {tally}" if not bad
              else f"FAIL {len(bad)} of {len(before)} before the fork did not land: " + "; ".join(describe(a) for a in bad[:4]))

    elif check == "inflight":
        late = [a for a in attempts if a["outcome"] == "landed" and a.get("builtFor") is not None
                and a["builtFor"] < floor and a.get("blockHeight") is not None and a["blockHeight"] >= fork_height]
        # The wallet learns of the fork from the indexer, so for a while after the fork block
        # it still builds for the old ledger. Those and the ones under way at the fork block
        # are the transfers the fork caught.
        caught = across + [a for a in after if a.get("builtFor") is not None and not is_new(a) and a not in across]
        hung = [a for a in caught if a["outcome"] == "timeout"]
        orphaned = len(soak.get("orphaned", []))
        if late:
            print(f"FAIL a transaction built for ledger {old} landed after the fork block: " + "; ".join(f"#{a['i']} at #{a['blockHeight']}" for a in late))
        elif hung:
            print("FAIL no verdict for transfers in flight at the fork: " + "; ".join(describe(a) for a in hung))
        elif not caught:
            print(f"WARN the fork block (#{fork_height}) caught no transfer; lower LOAD_INTERVAL_SECS")
        else:
            tally = ", ".join(f"{n} {o}" for o, n in Counter(a["outcome"] for a in caught).items())
            print(f"PASS the fork block (#{fork_height}) caught {len(caught)} transfers built for ledger {old}: {tally}; "
                  f"none hung, none landed after it; first: {describe(caught[0])}; {orphaned} reported Orphaned by the wallet")

    elif check == "recovered":
        landed = [a for a in after if a["outcome"] == "landed" and is_new(a)]
        regs = soak.get("registrations", [])
        reg = "; DUST re-registration " + ", ".join("landed" if r.get("ok") else f"failed ({(r.get('error') or {}).get('message', r.get('outcome', ''))[:80]})" for r in regs) if regs else ""
        if landed:
            first = landed[0]
            secs = (ms(first["endedAt"]) - fork_ms) // 1000
            print(f"PASS first ledger-{new} transfer #{first['i']} landed at #{first['blockHeight']}, {secs} s after the fork block{reg}")
        elif regs and not any(r.get("ok") for r in regs):
            print(f"FAIL no ledger-{new} transfer landed: the wallet could not re-register for DUST{reg}")
        else:
            tail = "; ".join(describe(a) for a in after[-3:]) or "no attempt after the fork"
            print(f"FAIL no ledger-{new} transfer landed in the window{reg}; last: {tail}")

    elif check == "history":
        h = soak.get("history", {})
        eras = h.get("byEra", {})
        want_new = any(a["outcome"] == "landed" and is_new(a) for a in attempts)
        ok = eras.get(f"v{old}", 0) > 0 and (eras.get(f"v{new}", 0) > 0 or not want_new)
        print(f"{'PASS' if ok else 'FAIL'} {h.get('total', 0)} entries; by era {json.dumps(eras)}; by lifecycle {json.dumps(h.get('byLifecycle', {}))}")

    else:
        print(f"FAIL unknown check {check}")


if __name__ == "__main__":
    main()
