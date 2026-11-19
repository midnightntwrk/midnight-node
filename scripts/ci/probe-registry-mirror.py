#!/usr/bin/env python3
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

"""Exercise mirror resolution and a deterministic mirror miss on isolated builders."""

import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
from urllib.parse import urlsplit


BUSYBOX = "busybox:1.35.0-uclibc@sha256:e5877866756a3714cd11309f0b34e7c2c62406fdbfcae39c96beabb5dd4005a7"
ALLOWED_HOSTS = {"mirror.gcr.io", "registry-1.docker.io", "registry.hub.docker.com"}


def requests(log):
    records = []
    for line in log.splitlines():
        try:
            fields = dict(part.split("=", 1) for part in shlex.split(line) if "=" in part)
            parsed = urlsplit(fields.get("url", ""))
            if parsed.hostname not in ALLOWED_HOSTS:
                continue
            records.append({
                "host": parsed.hostname,
                "path": parsed.path,
                "status": fields.get("response.status", ""),
            })
        except ValueError:
            continue
    return records


def verify(records, fallback):
    artifacts = [record for record in records
                 if "/busybox/manifests/" in record["path"] or "/busybox/blobs/" in record["path"]]
    mirror = [record for record in artifacts if record["host"] == "mirror.gcr.io"]
    if fallback:
        misses = [record for record in mirror
                  if "midnight-mirror-probe-missing" in record["path"]
                  and record["status"].startswith(("4", "5"))]
        upstream = [record for record in artifacts if record["host"] != "mirror.gcr.io"
                    and record["status"].startswith("2")]
        if not misses or not upstream:
            raise RuntimeError("Mirror miss followed by successful Hub fallback was not observed")
    elif not any(record["status"].startswith("2") for record in mirror):
        raise RuntimeError("Successful mirror.gcr.io artifact resolution was not observed")


def probe(repository, fallback):
    installation = "earth-mirror-fallback" if fallback else "earth-mirror-probe"
    container = installation + "-buildkitd"
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "Earthfile").write_text(
            "VERSION 0.8\nprobe:\n    FROM " + BUSYBOX + "\n    RUN echo registry-probe-ok\n"
        )
        config = (repository / ".earthly/config.hosted.yml").read_text()
        if fallback:
            # A separate namespace forces a cache miss even if Google later caches this digest.
            config = config.replace('"mirror.gcr.io"', '"mirror.gcr.io/midnight-mirror-probe-missing"')
        config += ("    tls_enabled: false\n"
                   "    buildkit_additional_args: [\"-m\", \"4Gb\"]\n"
                   "    local_registry_host: \"tcp://127.0.0.1:" + ("8392" if fallback else "8391") + "\"\n")
        (root / "config.yml").write_text(config)
        docker_config = root / "docker"
        docker_config.mkdir()
        (docker_config / "config.json").write_text("{}")
        env = {**os.environ, "DOCKER_CONFIG": str(docker_config)}
        for key in ("GITHUB_TOKEN", "GH_TOKEN", "NODE_GIT_TOKEN", "EARTHLY_TOKEN",
                    "EARTH_TOKEN", "DOCKER_AUTH_CONFIG", "EARTHLY_BUILD_ARGS"):
            env.pop(key, None)
        try:
            subprocess.run([
                "earth", "--debug", "--no-output", "--installation-name", installation,
                "--config", str(root / "config.yml"), "+probe",
            ], cwd=root, env=env, check=True)
            log = subprocess.run(["docker", "logs", container], text=True,
                                 capture_output=True, check=True)
            records = requests(log.stdout + log.stderr)
            # Raw HTTP debug logs can contain credentials or signed URLs; emit only these fields.
            print(json.dumps({"probe": installation, "requests": records}, indent=2))
            verify(records, fallback)
        finally:
            subprocess.run(["docker", "rm", "-f", container], check=False, capture_output=True)
            subprocess.run(["docker", "volume", "rm", installation + "-cache"],
                           check=False, capture_output=True)


if __name__ == "__main__":
    repository = Path(__file__).resolve().parents[2]
    probe(repository, fallback=False)
    probe(repository, fallback=True)
