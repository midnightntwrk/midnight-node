#!/usr/bin/env python3
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

"""Enable the public Docker Hub cache without restarting the hosted daemon."""

import argparse
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import time


MIRROR = "https://mirror.gcr.io"


def command(args):
    return subprocess.run(args, check=True, text=True, capture_output=True).stdout.strip()


def normalized(mirrors):
    return [mirror.rstrip("/") for mirror in mirrors]


def effective_mirrors():
    mirrors = json.loads(command(["docker", "info", "--format", "{{json .RegistryConfig.Mirrors}}"]))
    if mirrors is None:
        return []
    if not isinstance(mirrors, list) or not all(isinstance(item, str) for item in mirrors):
        raise ValueError("Docker returned an invalid registry mirror list")
    return normalized(mirrors)


def configure(config):
    config = Path(config)
    if config.is_symlink() or config.parent.is_symlink():
        raise ValueError("Docker configuration must not be a symbolic link")
    metadata = config.stat() if config.exists() else None
    if metadata and not stat.S_ISREG(metadata.st_mode):
        raise ValueError("Docker configuration must be a regular file")
    original = json.loads(config.read_text()) if metadata else {}
    if not isinstance(original, dict):
        raise ValueError("Docker configuration must be a JSON object")
    previous = original.get("registry-mirrors", [])
    if not isinstance(previous, list) or not all(isinstance(item, str) for item in previous):
        raise ValueError("registry-mirrors must be an array of URLs")
    desired = dict(original)
    desired["registry-mirrors"] = list(dict.fromkeys([MIRROR, *normalized(previous)]))
    expected = desired["registry-mirrors"]
    if desired == original and effective_mirrors() == expected:
        return False
    if command(["systemctl", "show", "docker.service", "--property=CanReload", "--value"]) != "yes":
        raise RuntimeError("Docker does not support reload; no configuration was changed")

    config.parent.mkdir(parents=True, exist_ok=True)
    candidate = None
    try:
        descriptor, name = tempfile.mkstemp(prefix=".daemon-mirror-", suffix=".json", dir=config.parent)
        candidate = Path(name)
        with os.fdopen(descriptor, "w") as stream:
            json.dump(desired, stream, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        if metadata:
            os.chown(candidate, metadata.st_uid, metadata.st_gid)
        os.chmod(candidate, stat.S_IMODE(metadata.st_mode) if metadata else 0o644)
        command(["dockerd", "--validate", "--config-file", str(candidate)])
        os.replace(candidate, config)
    finally:
        if candidate and candidate.exists():
            candidate.unlink()

    # Docker reloads registry-mirrors on SIGHUP; restarting would stop existing containers.
    command(["systemctl", "reload", "docker.service"])
    for _ in range(20):
        if effective_mirrors() == expected:
            print("Docker registry mirror is active: https://mirror.gcr.io")
            return True
        time.sleep(0.1)
    raise RuntimeError("Docker reload did not activate the configured registry mirrors")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", default="/etc/docker/daemon.json")
    args = parser.parse_args()
    configure(args.config)


if __name__ == "__main__":
    main()
