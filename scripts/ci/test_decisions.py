# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import scope
import trusted_cache

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('faucet', ROOT / 'scripts/ci/faucet-preflight.py')
faucet = importlib.util.module_from_spec(spec)
spec.loader.exec_module(faucet)


class ScopeTests(unittest.TestCase):
    def test_docs(self):
        self.assertTrue(scope.documentation_only(['README.md', 'docs/a.md', 'changes/bug/a.md']))

    def test_product_and_unknown_inputs(self):
        for path in ['runtime/a.rs', 'Cargo.lock', '.github/workflows/a.yml',
                     'docs/a.json', 'indexer', 'Earthfile', 'scripts/ci/scope.py']:
            with self.subTest(path=path):
                self.assertFalse(scope.documentation_only(['README.md', path]))
        self.assertFalse(scope.documentation_only([]))

    def test_failed_diff_runs_product_checks(self):
        with tempfile.TemporaryDirectory() as temp:
            event, output = Path(temp) / 'event', Path(temp) / 'output'
            event.write_text('{"pull_request":{"base":{"sha":"missing"}}}')
            with patch.dict(os.environ, GITHUB_EVENT_PATH=str(event), GITHUB_OUTPUT=str(output),
                            GITHUB_EVENT_NAME='pull_request'), patch.object(
                                scope.subprocess, 'check_output', side_effect=subprocess.CalledProcessError(1, 'git')):
                scope.main()
            self.assertEqual(output.read_text(), 'product=true\n')


class ImagePlanTests(unittest.TestCase):
    def plan(self, available, kind='main', **extra):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)
            docker = path / 'docker'
            docker.write_text('#!/bin/bash\ncase "$4" in *toolkit*) [[ "$AVAILABLE" == all ]];; *) [[ "$AVAILABLE" != none ]];; esac || exit 1\nprintf "sha256:%064d\\n" 1\n')
            docker.chmod(0o755)
            output = path / 'output'
            env = dict(os.environ, PATH=f'{temp}:{os.environ["PATH"]}', AVAILABLE=available,
                       GITHUB_REPOSITORY='midnightntwrk/midnight-node', GITHUB_OUTPUT=str(output), **extra)
            result = subprocess.run(['bash', str(ROOT / 'scripts/ci/image-plan.sh'), kind],
                                    cwd=ROOT, env=env, text=True, capture_output=True)
            return result, output.read_text() if output.exists() else ''

    def test_main_needs_both_images(self):
        for available in ['none', 'node']:
            result, output = self.plan(available)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('runner=["self-hosted","tier:large"]', output)
        result, output = self.plan('all')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('runner=["ubuntu-24.04"]', output)
        self.assertIn('@sha256:', output)

    def test_bot_only_needs_node(self):
        result, output = self.plan('node', 'bot')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('runner=["ubuntu-24.04"]', output)

    def test_force_build_and_missing_explicit_version(self):
        result, output = self.plan('all', FORCE_REBUILD='true')
        self.assertIn('runner=["self-hosted","tier:large"]', output)
        result, _ = self.plan('none', 'bot', VERSION='missing')
        self.assertNotEqual(result.returncode, 0)


class FaucetTests(unittest.TestCase):
    def ready(self, amounts):
        return faucet.funding_ready([{'value': {'ada': {'lovelace': amount}}} for amount in amounts],
                                    16, 250_000_000, 5_000_000)[0]

    def test_existing_workers_need_no_prime_fee(self):
        self.assertTrue(self.ready([250_000_000] * 16))

    def test_priming_and_observed_insufficient_balance(self):
        self.assertTrue(self.ready([4_005_000_000]))
        self.assertFalse(self.ready([4_004_999_999]))
        self.assertFalse(self.ready([3_936_000_000]))
        self.assertFalse(self.ready([]))

    def test_bad_amount_is_an_error(self):
        for amount in [-1, '250000000', True]:
            with self.assertRaises(ValueError):
                self.ready([amount])

    def test_source_settings(self):
        with patch('os.getcwd', return_value=str(ROOT)):
            endpoint, address, minimum, buffer = faucet.settings()
        self.assertTrue(endpoint.startswith('wss://'))
        self.assertTrue(address.startswith('addr_test'))
        self.assertEqual((minimum, buffer), (250_000_000, 5_000_000))


class TrustedCacheTests(unittest.TestCase):
    def test_exact_key_and_trusted_branch(self):
        def no_lookup(number):
            self.fail('Branch caches do not need a PR lookup')
        self.assertTrue(trusted_cache.trusted_hit(
            [{'key': 'signed', 'ref': 'refs/heads/main'}], 'signed', 'org/repo', no_lookup))
        self.assertFalse(trusted_cache.trusted_hit(
            [{'key': 'signed-spoof', 'ref': 'refs/heads/main'}], 'signed', 'org/repo', no_lookup))

    def test_fork_marker_cannot_satisfy_trusted_check(self):
        entry = [{'key': 'signed', 'ref': 'refs/pull/42/merge'}]
        self.assertFalse(trusted_cache.trusted_hit(entry, 'signed', 'org/repo', lambda _: 'fork/repo'))
        self.assertTrue(trusted_cache.trusted_hit(entry, 'signed', 'org/repo', lambda _: 'org/repo'))

    def test_unknown_ref_is_not_trusted(self):
        self.assertFalse(trusted_cache.trusted_hit(
            [{'key': 'signed', 'ref': 'refs/tags/v1'}], 'signed', 'org/repo', lambda _: 'org/repo'))

    def test_api_failure_is_not_a_hit(self):
        def unavailable(number):
            raise subprocess.CalledProcessError(1, 'gh')
        with self.assertRaises(subprocess.CalledProcessError):
            trusted_cache.trusted_hit([{'key': 'signed', 'ref': 'refs/pull/42/merge'}],
                                      'signed', 'org/repo', unavailable)
