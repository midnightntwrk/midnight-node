# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
PUBLIC = "midnightntwrk/midnight-node-ci"
PRIVATE = "ghcr.io/midnight-ntwrk/midnight-node-ci"


class RegistrySelection(unittest.TestCase):
    def test_workflow_trust_boundaries(self):
        # These selectors use only operators shared by Actions expressions and JavaScript.
        for name in ("continuous-integration.yml", "continuous-integration-test.yml",
                     "continuous-integration-checks.yml"):
            text = (ROOT / ".github/workflows" / name).read_text()
            expression = re.search(r"^  NODE_CI_IMAGE_REPO: \$\{\{ (.*) \}\}$", text, re.M)[1]
            cases = [
                ("pull_request", "midnightntwrk/midnight-node", "midnightntwrk/midnight-node", True, PRIVATE),
                ("pull_request", "midnightntwrk/midnight-node", "someone/fork", True, PUBLIC),
                ("pull_request", "someone/clone", "someone/clone", True, PUBLIC),
                ("pull_request", "midnightntwrk/midnight-node", "midnightntwrk/midnight-node", False, PUBLIC),
                ("push", "midnightntwrk/midnight-node", None, True, PRIVATE),
                ("merge_group", "midnightntwrk/midnight-node", None, True, PRIVATE),
                ("workflow_dispatch", "someone/clone", None, True, PUBLIC),
            ]
            for event, repository, head, authenticated, expected in cases:
                with self.subTest(workflow=name, event=event, repository=repository,
                                  head=head, authenticated=authenticated):
                    github = {"event_name": event, "repository": repository, "event": {}}
                    if head:
                        github["event"] = {"pull_request": {"head": {"repo": {"full_name": head}}}}
                    secrets = {key: "test-only" if authenticated else "" for key in
                               ("MIDNIGHTCI_PACKAGES_READ", "MIDNIGHTCI_PACKAGES_WRITE")}
                    result = subprocess.check_output([
                        "node", "-e",
                        'const [expr,g,s]=process.argv.slice(1); process.stdout.write('
                        'Function("github","secrets","return ("+expr+")")'
                        '(JSON.parse(g),JSON.parse(s)));',
                        expression, json.dumps(github), json.dumps(secrets),
                    ], text=True)
                    self.assertEqual(result, expected)

    def test_envrc_forwards_only_explicit_override_and_preserves_arguments(self):
        for override in (None, PUBLIC, PRIVATE):
            with self.subTest(override=override), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                subprocess.run(["git", "init", "-q", directory], check=True)
                (root / ".envrc").write_text((ROOT / ".envrc").read_text())
                (root / "rust-toolchain.toml").write_text((ROOT / "rust-toolchain.toml").read_text())
                env = {"PATH": os.environ["PATH"], "CI": "true",
                       "EARTHLY_BUILD_ARGS": "CACHE_KEY=test"}
                if override is not None:
                    env["NODE_CI_IMAGE_REPO"] = override
                output = subprocess.check_output([
                    "bash", "-c", '. ./.envrc >/dev/null && printf "%s" "$EARTHLY_BUILD_ARGS"'
                ], cwd=root, env=env, text=True)
                expected = "CACHE_KEY=test,CI=true"
                if override:
                    expected += ",NODE_CI_IMAGE_REPO=" + override
                self.assertEqual(output, expected)


if __name__ == "__main__":
    unittest.main()
