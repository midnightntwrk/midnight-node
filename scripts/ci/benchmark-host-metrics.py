#!/usr/bin/env python3
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http:#www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Record host counters, not process arguments or environment variables."""
import json
import os
from pathlib import Path
import time

while True:
    memory = {}
    for line in Path('/proc/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        if key in ('MemTotal', 'MemAvailable'):
            memory[key] = int(value.split()[0]) * 1024
    workers = 0
    for entry in Path('/proc').iterdir():
        if entry.name.isdigit():
            try:
                workers += (entry / 'comm').read_text().strip() == 'Runner.Worker'
            except (OSError, ProcessLookupError):
                pass
    disk = os.statvfs('/var/lib/docker')
    print(json.dumps({
        'epoch': time.time(), 'load': os.getloadavg(),
        'cpu_ticks': Path('/proc/stat').read_text().splitlines()[0],
        'memory_bytes': memory, 'runner_workers': workers,
        'disk_available_bytes': disk.f_bavail * disk.f_frsize,
    }), flush=True)
    time.sleep(10)
