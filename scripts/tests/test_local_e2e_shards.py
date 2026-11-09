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

"""Validate e2e coverage selection without compiling or running the chain."""

import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "local_e2e_shard", Path(__file__).with_name("run-local-e2e-shard.py")
)
sharder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sharder)


class LocalE2EShardsTest(unittest.TestCase):
    def setUp(self):
        self.core = {
            "contract_state::contract_state_for_undeployed_address_returns_not_present",
            "contract_state::contract_state_distinguishes_historical_and_current_blocks",
            "operational::valid_deploy_transaction_succeeds_via_rpc",
            "operational::toolkit_multi_dest_send_does_not_hang",
            "cnight::observation::register_for_dust_production",
            *sharder.EXPECTED_IGNORED,
        }
        self.other_bridge = {
            "c2m_bridge::bridge_transfer_cnight_to_midnight_address",
            "c2m_bridge::unapproved_cardano_tx_makes_transfer_that_unlocks_to_treasury",
            "c2m_bridge::subminimal_transfers_accumulate_and_flush_on_threshold_breach",
        }
        self.inventory = self.core | self.other_bridge | sharder.RECIPIENT_TESTS
        self.command = [
            "with-git-creds", "cargo", "test", "--test", "e2e_tests",
            "--no-default-features", "--features", "local",
        ]

    def partitions(self, names=None, ignored=None):
        return sharder.partition_inventory(
            self.inventory if names is None else names,
            sharder.EXPECTED_IGNORED if ignored is None else ignored,
        )

    def test_partitions_cover_every_test_once_and_keep_deploy_gate_together(self):
        partitions = self.partitions()
        self.assertEqual(partitions["default"], self.core | self.other_bridge)
        self.assertEqual(set().union(*partitions.values()), self.inventory)
        self.assertEqual(sum(map(len, partitions.values())), len(self.inventory))
        self.assertEqual(len(partitions["bridge-recipient"]), 2)

    def test_new_nonbridge_test_is_automatically_covered(self):
        name = "new_module::new_test"
        self.assertIn(name, self.partitions(self.inventory | {name})["default"])

    def test_new_bridge_test_is_automatically_covered_by_default(self):
        name = "c2m_bridge::new_bridge_test"
        self.assertIn(name, self.partitions(self.inventory | {name})["default"])

    def test_removed_recipient_test_cannot_make_a_filter_silently_empty(self):
        missing = "c2m_bridge::bridge_transfer_invalid_recipient_unlocks_to_treasury"
        with self.assertRaisesRegex(ValueError, "Recipient tests are missing"):
            self.partitions(self.inventory - {missing})

    def test_ignore_changes_are_not_silently_accepted(self):
        with self.assertRaisesRegex(ValueError, "ignored tests changed"):
            self.partitions(ignored=sharder.EXPECTED_IGNORED | {"new::ignored"})
        with self.assertRaisesRegex(ValueError, "missing from the complete"):
            self.partitions(self.inventory - {"operational::consolidate_faucet"})

    def test_inventory_format_and_duplicate_fail_closed(self):
        self.assertEqual(sharder.parse_inventory("a::b: test\n\n1 test, 0 benchmarks\n"), {"a::b"})
        for output in ["", "a: test\na: test\n", "a: benchmark\n", "not a test listing"]:
            with self.subTest(output=output), self.assertRaises(ValueError):
                sharder.parse_inventory(output)

    def test_execution_uses_exact_libtest_filters_and_propagates_failure(self):
        selected = self.partitions()["bridge-recipient"]
        with patch.object(sharder, "list_tests", side_effect=[
            self.inventory, sharder.EXPECTED_IGNORED, selected,
        ]) as listing, patch.object(sharder.subprocess, "run") as run:
            run.return_value.returncode = 101
            self.assertEqual(sharder.run_shard("bridge-recipient", self.command), 101)
        self.assertEqual(listing.call_args.args[1:], ("--exact", *sorted(selected)))
        self.assertEqual(run.call_args.args[0], [
            *self.command, "--", "--test-threads=6", "--nocapture", "--exact", *sorted(selected),
        ])
        self.assertNotIn("--include-ignored", run.call_args.args[0])
        self.assertNotIn("--ignored", run.call_args.args[0])

    def test_all_selects_the_complete_inventory_without_changing_ignores(self):
        with patch.object(sharder, "list_tests", side_effect=[
            self.inventory, sharder.EXPECTED_IGNORED, self.inventory,
        ]), patch.object(sharder.subprocess, "run") as run:
            run.return_value.returncode = 0
            self.assertEqual(sharder.run_shard("all", self.command), 0)
        self.assertEqual(run.call_args.args[0][-len(self.inventory):], sorted(self.inventory))

    def test_incorrect_harness_selection_never_runs_tests(self):
        with patch.object(sharder, "list_tests", side_effect=[
            self.inventory, sharder.EXPECTED_IGNORED, self.core,
        ]), patch.object(sharder.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "exactly the requested"):
                sharder.run_shard("bridge-recipient", self.command)
        run.assert_not_called()

    def test_inventory_process_failure_is_not_replaced_with_empty_success(self):
        failure = subprocess.CalledProcessError(37, self.command)
        with patch.object(sharder, "list_tests", side_effect=failure):
            self.assertEqual(sharder.main(["default", "--", *self.command]), 37)

    def test_embedded_libtest_options_are_rejected(self):
        with patch.object(sharder, "list_tests") as listing:
            with self.assertRaisesRegex(ValueError, "without libtest arguments"):
                sharder.run_shard("all", [*self.command, "--", "--ignored"])
        listing.assert_not_called()


if __name__ == "__main__":
    unittest.main()
