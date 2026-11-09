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

"""Run one local e2e partition against its own already-started local chain."""

import argparse
import re
import subprocess
import sys

RECIPIENT_TESTS = {
    "c2m_bridge::bridge_transfer_invalid_recipient_unlocks_to_treasury",
    "c2m_bridge::approved_bridge_transfer_with_short_recipient_keeps_the_bridge_live",
}
EXPECTED_IGNORED = {
    "cnight::observation::create_hundred_registrations",
    "operational::consolidate_faucet",
}
SHARDS = ("all", "default", "bridge-recipient")


def parse_inventory(output):
    """Reject incomplete or unfamiliar libtest output rather than lose coverage."""
    names = set()
    for line in output.splitlines():
        line = line.strip()
        if not line or re.fullmatch(r"\d+ tests?, 0 benchmarks?", line):
            continue
        match = re.fullmatch(r"(\S+): test", line)
        if not match:
            raise ValueError(f"Unexpected libtest inventory line: {line!r}")
        name = match.group(1)
        if name in names:
            raise ValueError(f"Duplicate test in libtest inventory: {name}")
        names.add(name)
    if not names:
        raise ValueError("The local e2e test inventory is empty")
    return names


def partition_inventory(names, ignored):
    if ignored != EXPECTED_IGNORED:
        raise ValueError(
            "Local e2e ignored tests changed: "
            f"expected {sorted(EXPECTED_IGNORED)}, got {sorted(ignored)}"
        )
    if not ignored <= names:
        raise ValueError("Ignored tests are missing from the complete inventory")

    if not RECIPIENT_TESTS <= names:
        raise ValueError(
            "Recipient tests are missing from the local inventory: "
            f"{sorted(RECIPIENT_TESTS - names)}"
        )

    # The complement keeps new tests and the shared pre-deploy gate together.
    partitions = {
        "default": names - RECIPIENT_TESTS,
        "bridge-recipient": RECIPIENT_TESTS,
    }
    assigned = set()
    for shard, tests in partitions.items():
        if not tests:
            raise ValueError(f"Empty local e2e shard: {shard}")
        if assigned & tests:
            raise ValueError(f"Overlapping local e2e shard: {shard}")
        assigned.update(tests)
    if assigned != names:
        raise ValueError("Local e2e partitions do not cover the complete inventory")
    return partitions


def list_tests(command, *arguments):
    result = subprocess.run(
        [*command, "--", "--list", "--format", "terse", *arguments],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    return parse_inventory(result.stdout)


def run_shard(shard, command):
    if not command or "--" in command:
        raise ValueError("Provide a cargo test command without libtest arguments")
    names = list_tests(command)
    ignored = list_tests(command, "--ignored")
    partitions = partition_inventory(names, ignored)
    selected = names if shard == "all" else partitions[shard]
    # Filters belong after Cargo's separator; libtest ORs the exact full names.
    filters = ["--exact", *sorted(selected)]
    if list_tests(command, *filters) != selected:
        raise ValueError("Libtest filters do not select exactly the requested shard")
    print(
        f"Local e2e inventory: {len(names)} tests, {len(ignored)} ignored; "
        f"shard {shard}: {len(selected)} tests, {len(selected & ignored)} ignored",
        flush=True,
    )
    return subprocess.run(
        [*command, "--", "--test-threads=6", "--nocapture", *filters],
        check=False,
    ).returncode


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shard", choices=SHARDS)
    parser.add_argument(
        "command", nargs=argparse.REMAINDER, help="cargo test command after --"
    )
    args = parser.parse_args(argv)
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    try:
        return run_shard(args.shard, command)
    except (ValueError, OSError) as error:
        print(f"Local e2e shard validation failed: {error}", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as error:
        print("Local e2e inventory command failed", file=sys.stderr)
        return error.returncode


if __name__ == "__main__":
    sys.exit(main())
