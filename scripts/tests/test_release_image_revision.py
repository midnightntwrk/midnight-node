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

"""Run with python3 -m unittest discover -s scripts/tests -p test_release_image_revision.py."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/check-image-revision.sh"
MAIN = yaml.safe_load((ROOT / ".github/workflows/main.yml").read_text())
RELEASE = yaml.safe_load((ROOT / ".github/workflows/release-image.yml").read_text())


def step(workflow, job, name):
    return next(s for s in workflow["jobs"][job]["steps"] if s.get("name") == name)


class ReleaseRevisionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cwd = Path(self.tmp.name)
        (self.cwd / "bin").mkdir()
        docker = self.cwd / "bin/docker"
        docker.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$DOCKER_CALLS"\n'
                          '[ "${DOCKER_FAIL:-false}" != true ] || exit 1\n'
                          'printf "%s\\n" "$IMAGE_CONFIG"\n')
        docker.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.cwd / "bin") + os.pathsep + os.environ["PATH"],
                        DOCKER_CALLS=str(self.cwd / "calls"), GITHUB_OUTPUT=str(self.cwd / "output"),
                        GITHUB_ENV=str(self.cwd / "env"), GITHUB_REPOSITORY="midnightntwrk/midnight-node",
                        GHCR_REGISTRY="ghcr.io/midnight-ntwrk", GHCR_REGISTRY_PUBLIC="ghcr.io/midnightntwrk",
                        FORCE_REBUILD="false", SUFFIX="-rc.1")
        self.git("init", "-q")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.invalid")
        for path, text in {"node/Cargo.toml": 'version = "1.0.400"\n',
                           "util/toolkit/Cargo.toml": 'version = "1.0.400"\n',
                           "runtime/src/lib.rs": 'spec_version: 001_000_300,\n'}.items():
            p = self.cwd / path
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
        self.git("add", ".")
        self.git("-c", "commit.gpgsign=false", "commit", "-qm", "PR merge")
        self.pr_sha = self.git("rev-parse", "HEAD")
        self.git("-c", "commit.gpgsign=false", "commit", "--allow-empty", "-qm", "Release merge")
        self.release_sha = self.git("rev-parse", "HEAD")
        for path in ["scripts/check-image-revision.sh", ".release-workflow/scripts/check-image-revision.sh"]:
            dest = self.cwd / path
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(HELPER, dest)
        self.config(self.release_sha)

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.cwd, stderr=subprocess.DEVNULL, text=True).strip()

    def config(self, sha, platforms=("linux/amd64", "linux/arm64")):
        self.env["IMAGE_CONFIG"] = json.dumps({p: {"config": {"Labels": {
            "org.opencontainers.image.revision": sha}}} for p in platforms})

    def run_shell(self, script):
        return subprocess.run(["bash", "-euo", "pipefail", "-c", script],
                              cwd=self.cwd, env=self.env, capture_output=True, text=True)

    def outputs(self):
        return dict(line.split("=", 1) for line in (self.cwd / "output").read_text().splitlines())

    def test_same_tree_commit_build_is_not_reused(self):
        self.assertEqual(self.git("rev-parse", self.pr_sha + "^{tree}"),
                         self.git("rev-parse", self.release_sha + "^{tree}"))
        for arch in ("amd64", "arm64"):
            script = step(MAIN, "publish-" + arch, "Check if images already exist")["run"]
            self.config(self.pr_sha)
            self.assertEqual(self.run_shell(script).returncode, 0)
            self.assertEqual(self.outputs()["images_exist"], "false")
            self.assertEqual(self.outputs()["IMAGE_TAG"], "1.0.400-" + self.release_sha)
            self.config(self.release_sha)
            self.assertEqual(self.run_shell(script).returncode, 0)
            self.assertEqual(self.outputs()["images_exist"], "true")
        self.env["FORCE_REBUILD"] = "true"
        self.assertEqual(self.run_shell(script).returncode, 0)
        self.assertEqual(self.outputs()["images_exist"], "false")

    def test_missing_wrong_or_unavailable_revision_fails_closed(self):
        script = f'bash scripts/check-image-revision.sh example:tag {self.release_sha} linux/arm64'
        for config in ["{}", "null", "not-json", json.dumps({"linux/arm64": {"config": {}}})]:
            self.env["IMAGE_CONFIG"] = config
            self.assertNotEqual(self.run_shell(script).returncode, 0)
        self.config(self.pr_sha)
        self.assertNotEqual(self.run_shell(script).returncode, 0)
        self.config(self.release_sha, ("linux/amd64",))
        self.assertNotEqual(self.run_shell(script).returncode, 0)
        self.config(self.release_sha)
        self.assertEqual(self.run_shell(script).returncode, 0)
        self.env["DOCKER_FAIL"] = "true"
        self.assertNotEqual(self.run_shell(script).returncode, 0)

    def test_abbreviated_revision_is_rejected(self):
        self.assertNotEqual(self.run_shell(
            f'bash scripts/check-image-revision.sh example:tag {self.release_sha[:8]} linux/amd64'
        ).returncode, 0)

    def test_single_architecture_manifest_config(self):
        self.env["IMAGE_CONFIG"] = json.dumps({"os": "linux", "architecture": "amd64",
            "config": {"Labels": {"org.opencontainers.image.revision": self.release_sha}}})
        script = f'bash scripts/check-image-revision.sh example:tag {self.release_sha}'
        self.assertEqual(self.run_shell(script + " linux/amd64").returncode, 0)
        self.assertNotEqual(self.run_shell(script + " linux/arm64").returncode, 0)

    def test_release_resolves_checked_out_commit_not_workflow_sha(self):
        self.env["GITHUB_SHA"] = self.pr_sha
        result = self.run_shell(step(RELEASE, "prepare-release", "Setup Env")["run"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.outputs()["release_sha"], self.release_sha)
        self.assertEqual(self.outputs()["image_tag_commit"], "1.0.400-" + self.release_sha)
        self.assertEqual(self.outputs()["image_tag_release"], "1.0.400-rc.1")

    def test_release_checks_selected_components_and_both_architectures(self):
        self.env.update(RELEASE_SHA=self.release_sha, IMAGE_TAG_COMMIT="1.0.400-" + self.release_sha,
                        NODE_IMAGE="ghcr.io/midnight-ntwrk/midnight-node",
                        TOOLKIT_IMAGE="ghcr.io/midnight-ntwrk/midnight-node-toolkit")
        script = step(RELEASE, "prepare-release", "Verify release image revisions before publishing")["run"]
        for node, toolkit in [(False, False), (True, False), (False, True), (True, True)]:
            with self.subTest(skip_node=node, skip_toolkit=toolkit):
                self.env.update(SKIP_NODE=str(node).lower(), SKIP_TOOLKIT=str(toolkit).lower())
                Path(self.env["DOCKER_CALLS"]).write_text("")
                result = self.run_shell(script)
                self.assertEqual(result.returncode, 0, result.stderr)
                calls = Path(self.env["DOCKER_CALLS"]).read_text().splitlines()
                self.assertEqual(len(calls), 6 * (2 - node - toolkit))
                if calls:
                    self.assertTrue(any("-amd64" in c for c in calls))
                    self.assertTrue(any("-arm64" in c for c in calls))
        self.env.update(SKIP_NODE="false", SKIP_TOOLKIT="true")
        self.config(self.pr_sha)
        self.assertNotEqual(self.run_shell(script).returncode, 0)
        self.env.update(GITHUB_REPOSITORY="example/private-node", GHCR_REGISTRY_PUBLIC="ghcr.io/example",
                        NODE_IMAGE="ghcr.io/example/private-node")
        self.config(self.release_sha)
        Path(self.env["DOCKER_CALLS"]).write_text("")
        self.assertEqual(self.run_shell(script).returncode, 0)
        self.assertNotIn("midnightntwrk", Path(self.env["DOCKER_CALLS"]).read_text())

    def test_promotion_guard_precedes_all_publishing(self):
        steps = RELEASE["jobs"]["prepare-release"]["steps"]
        guard = next(i for i, s in enumerate(steps) if s.get("name") ==
                     "Verify release image revisions before publishing")
        for i, s in enumerate(steps):
            if "imagetools create" in s.get("run", "") or "docker push" in s.get("run", ""):
                self.assertGreater(i, guard)
        for job in ("publish-amd64", "publish-arm64", "publish-multi-arch"):
            checkout = step(MAIN, job, "Checkout node repository")
            self.assertEqual(checkout["with"]["ref"], "${{ needs.resolve-ref.outputs.commit-sha }}")
        self.assertEqual(step(RELEASE, "finalize-release", "Checkout repo")["with"]["ref"],
                         "${{ needs.prepare-release.outputs.release-sha }}")
        self.assertEqual(RELEASE["jobs"]["srtool-build"]["with"]["ref"],
                         "${{ needs.prepare-release.outputs.release-sha }}")
        for job, name in [("publish-amd64", "Run build"), ("publish-arm64", "Run Node and toolkit builds")]:
            self.assertEqual(step(MAIN, job, name)["run"].count('--IMAGE_TAG_HASH="$(git rev-parse HEAD)"'), 2)


if __name__ == "__main__":
    unittest.main()
