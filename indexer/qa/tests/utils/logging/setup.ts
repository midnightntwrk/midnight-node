// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { join } from 'path';
import { existsSync, mkdirSync, writeFileSync } from 'fs';
import { env } from '../../environment/model';

export default async () => {
  // Fail fast (with a clear message) if INDEXER_INSTANCE points at an
  // unrouted/not-yet-synced blue/green host. No-op when unset or undeployed.
  await env.preflightInstanceSelection();

  const BASE = 'logs';
  if (!existsSync(BASE)) {
    mkdirSync(BASE);
  }

  // timestamp folder name
  const ts = new Date().toISOString().replace(/T/, '_').replace(/:/g, '-').replace(/\..+/, '');

  const sessionDir = join(BASE, ts);
  // Handle parallel execution - directory may already exist from another project
  if (!existsSync(sessionDir)) {
    mkdirSync(sessionDir);
  }

  // write the path your logger will read
  writeFileSync(join(BASE, 'sessionPath'), sessionDir, 'utf8');
};
