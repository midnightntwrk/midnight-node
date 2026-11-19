# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest
from unittest.mock import patch


MODULE = Path(__file__).with_name("configure-docker-mirror.py")
SPEC = importlib.util.spec_from_file_location("docker_mirror", MODULE)
mirror = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mirror)
PROBE_SPEC = importlib.util.spec_from_file_location("mirror_probe", MODULE.with_name("probe-registry-mirror.py"))
probe = importlib.util.module_from_spec(PROBE_SPEC)
PROBE_SPEC.loader.exec_module(probe)
ROOT = Path(__file__).resolve().parents[2]


class DockerMirror(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.config = Path(self.directory.name) / "daemon.json"
        self.calls = []
        self.live = []
        self.reloadable = "yes"
        self.validation_error = False
        self.reload_error = False
        self.ignore_reload = False
        self.patcher = patch.object(mirror, "command", self.command)
        self.patcher.start()
        self.addCleanup(self.patcher.stop)
        self.sleep = patch.object(mirror.time, "sleep")
        self.sleep.start()
        self.addCleanup(self.sleep.stop)

    def command(self, args):
        self.calls.append(args)
        if args[:2] == ["docker", "info"]:
            return json.dumps(self.live)
        if args[:2] == ["systemctl", "show"]:
            return self.reloadable
        if args[:2] == ["dockerd", "--validate"]:
            candidate = json.loads(Path(args[-1]).read_text())
            self.assertIn(mirror.MIRROR, candidate["registry-mirrors"])
            if self.validation_error:
                raise subprocess.CalledProcessError(1, args)
            return "configuration OK"
        if args == ["systemctl", "reload", "docker.service"]:
            if self.reload_error:
                raise subprocess.CalledProcessError(1, args)
            if not self.ignore_reload:
                self.live = json.loads(self.config.read_text())["registry-mirrors"]
            return ""
        self.fail("Unexpected command: " + str(args))

    def test_missing_configuration(self):
        self.assertTrue(mirror.configure(self.config))
        self.assertEqual(json.loads(self.config.read_text()), {"registry-mirrors": [mirror.MIRROR]})
        self.assertEqual(stat.S_IMODE(self.config.stat().st_mode), 0o644)
        self.assertFalse(any("restart" in args for args in self.calls))

    def test_preserves_options_mirrors_permissions_and_deduplicates(self):
        self.config.write_text(json.dumps({"log-driver": "local", "live-restore": True,
                                          "registry-mirrors": ["https://other.invalid/", mirror.MIRROR + "/"]}))
        self.config.chmod(0o600)
        mirror.configure(self.config)
        self.assertEqual(json.loads(self.config.read_text()), {
            "log-driver": "local", "live-restore": True,
            "registry-mirrors": [mirror.MIRROR, "https://other.invalid"],
        })
        self.assertEqual(stat.S_IMODE(self.config.stat().st_mode), 0o600)

    def test_idempotent_when_file_and_daemon_match(self):
        self.config.write_text(json.dumps({"registry-mirrors": [mirror.MIRROR]}))
        self.live = [mirror.MIRROR + "/"]
        self.assertFalse(mirror.configure(self.config))
        self.assertEqual(len(self.calls), 1)

    def test_matching_file_stale_daemon_reloads(self):
        self.config.write_text(json.dumps({"registry-mirrors": [mirror.MIRROR]}))
        self.assertTrue(mirror.configure(self.config))
        self.assertIn(["systemctl", "reload", "docker.service"], self.calls)

    def test_invalid_configuration_is_not_replaced(self):
        for content in ("{invalid", "[]", '{"registry-mirrors": "https://example.invalid"}',
                        '{"registry-mirrors": [1]}'):
            with self.subTest(content=content):
                self.config.write_text(content)
                with self.assertRaises((ValueError, json.JSONDecodeError)):
                    mirror.configure(self.config)
                self.assertEqual(self.config.read_text(), content)
        self.assertEqual(self.calls, [])

    def test_validation_failure_preserves_original(self):
        self.config.write_text('{"log-driver": "local"}')
        self.validation_error = True
        with self.assertRaises(subprocess.CalledProcessError):
            mirror.configure(self.config)
        self.assertEqual(self.config.read_text(), '{"log-driver": "local"}')
        self.assertFalse(any(args[:2] == ["systemctl", "reload"] for args in self.calls))
        self.assertEqual(list(self.config.parent.glob(".daemon-mirror-*")), [])

    def test_unsupported_reload_does_not_write(self):
        self.reloadable = "no"
        with self.assertRaisesRegex(RuntimeError, "does not support reload"):
            mirror.configure(self.config)
        self.assertFalse(self.config.exists())

    def test_failed_reload_does_not_restart(self):
        self.reload_error = True
        with self.assertRaises(subprocess.CalledProcessError):
            mirror.configure(self.config)
        self.assertFalse(any("restart" in args for args in self.calls))

    def test_ignored_reload_fails_verification(self):
        self.ignore_reload = True
        with self.assertRaisesRegex(RuntimeError, "did not activate"):
            mirror.configure(self.config)

    def test_symlink_configuration_is_not_replaced(self):
        target = self.config.with_name("elsewhere.json")
        target.write_text("{}")
        self.config.symlink_to(target)
        with self.assertRaisesRegex(ValueError, "symbolic link"):
            mirror.configure(self.config)
        self.assertTrue(self.config.is_symlink())
        self.assertEqual(target.read_text(), "{}")


class ProbeOutput(unittest.TestCase):
    def test_registry_records_exclude_credentials_queries_and_unrelated_hosts(self):
        log = ('time="2026-11-19" level=debug request.header.authorization="Bearer secret" '
               'url="https://user:password@mirror.gcr.io/v2/library/busybox/manifests/test?token=secret" '
               'response.status="200 OK"\n'
               'url="https://storage.invalid/private/object?signed=secret" response.status="200 OK"\n')
        self.assertEqual(probe.requests(log), [
            {"host": "mirror.gcr.io", "path": "/v2/library/busybox/manifests/test", "status": "200 OK"}
        ])

    def test_malformed_log_lines_are_ignored(self):
        self.assertEqual(probe.requests('url="unterminated\nplain line'), [])

    def test_registry_ping_or_token_success_is_not_a_mirror_hit(self):
        records = [
            {"host": "mirror.gcr.io", "path": "/v2/", "status": "200 OK"},
            {"host": "mirror.gcr.io", "path": "/v2/token", "status": "200 OK"},
            {"host": "mirror.gcr.io", "path": "/v2/library/busybox/manifests/pin", "status": "404 Not Found"},
            {"host": "registry-1.docker.io", "path": "/v2/library/busybox/manifests/pin", "status": "200 OK"},
        ]
        with self.assertRaisesRegex(RuntimeError, "artifact resolution"):
            probe.verify(records, fallback=False)
        records[2]["status"] = "200 OK"
        probe.verify(records, fallback=False)

    def test_fallback_requires_hub_artifact_success(self):
        records = [
            {"host": "mirror.gcr.io", "path": "/v2/midnight-mirror-probe-missing/library/busybox/manifests/pin",
             "status": "404 Not Found"},
            {"host": "registry-1.docker.io", "path": "/v2/", "status": "200 OK"},
        ]
        with self.assertRaisesRegex(RuntimeError, "successful Hub fallback"):
            probe.verify(records, fallback=True)
        records[1]["path"] = "/v2/library/busybox/manifests/pin"
        probe.verify(records, fallback=True)


class ConfigSelection(unittest.TestCase):
    def test_hosted_selfhost_local_unknown_and_missing_configs(self):
        cases = [
            ("github-hosted", True, True, "config.hosted.yml"),
            ("self-hosted", True, True, "config.selfhosted.yml"),
            (None, True, True, "config.yml"),
            ("unknown", True, True, "config.yml"),
            ("github-hosted", False, True, "config.yml"),
            ("self-hosted", True, False, "config.yml"),
        ]
        for environment, hosted, selfhost, expected in cases:
            with self.subTest(environment=environment, hosted=hosted, selfhost=selfhost):
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory).resolve()
                    subprocess.run(["git", "init", "-q", directory], check=True)
                    (root / ".envrc").write_text((ROOT / ".envrc").read_text())
                    (root / "rust-toolchain.toml").write_text((ROOT / "rust-toolchain.toml").read_text())
                    config = root / ".earthly"
                    config.mkdir()
                    (config / "config.yml").touch()
                    if hosted:
                        (config / "config.hosted.yml").touch()
                    if selfhost:
                        (config / "config.selfhosted.yml").touch()
                    env = {"PATH": os.environ["PATH"]}
                    if environment:
                        env["RUNNER_ENVIRONMENT"] = environment
                    output = subprocess.check_output([
                        "bash", "-c", '. ./.envrc >/dev/null && printf "%s" "$EARTHLY_CONFIG"'
                    ], cwd=root, env=env, text=True)
                    self.assertEqual(output, str(config / expected))


if __name__ == "__main__":
    unittest.main()
