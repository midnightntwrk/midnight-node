#!/usr/bin/env python3
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

import json
import os
from pathlib import PurePosixPath
import subprocess


def documentation_only(paths):
    return bool(paths) and all(
        path in {"README.md", "CONTRIBUTING.md"}
        or (PurePosixPath(path).parts[0] in {"docs", "changes", ".changes_archive"}
            and path.endswith(".md"))
        for path in paths
    )


def main():
    with open(os.environ["GITHUB_EVENT_PATH"]) as event_file:
        event = json.load(event_file)
    base = None
    if os.environ["GITHUB_EVENT_NAME"] == "pull_request":
        base = event["pull_request"]["base"]["sha"]
    elif os.environ["GITHUB_EVENT_NAME"] == "merge_group":
        base = event["merge_group"]["base_sha"]
    changed = True
    if base:
        try:
            result = subprocess.check_output(
                ["git", "diff", "--name-only", "--no-renames", "-z", base, "HEAD"],
            )
            paths = result.decode().rstrip("\0").split("\0") if result else []
            changed = not documentation_only(paths)
        except (subprocess.CalledProcessError, UnicodeDecodeError):
            pass
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"product={str(changed).lower()}\n")
    print("Product checks required" if changed else "Documentation-only change: no product inputs changed")


if __name__ == "__main__":
    main()
