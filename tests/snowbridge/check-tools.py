#!/usr/bin/env python3
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

import hashlib
import json
from pathlib import Path
import sys
root = Path(__file__).resolve().parents[2]
pins = json.loads((root / "tests/snowbridge/tool-archives.json").read_text())
pin = pins[sys.argv[1]]
archive = root / "artifacts/snowbridge/tools" / pin["file"]
actual = hashlib.sha256(archive.read_bytes()).hexdigest()
if actual != pin["sha256"]:
    raise SystemExit(f"Checksum mismatch for {archive}")
