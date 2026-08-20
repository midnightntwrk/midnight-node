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

import { UndeployedEnvironmentManager } from '../utils/undeployed/environment-manager';

// vitest globalSetup file for the integration suite. Always registered;
// no-ops outside `TARGET_ENV=undeployed`. Provisions the stack with pre-seeded
// node data and regenerates `qa/tests/data/static/undeployed/` via the block
// scanner (invoked by `qa/scripts/startup-localenv-with-data.sh`).

let manager: UndeployedEnvironmentManager | undefined;

export async function setup(): Promise<void> {
  if (process.env.TARGET_ENV !== 'undeployed') return;

  manager = new UndeployedEnvironmentManager({
    withData: true,
    flavour: process.env.FLAVOUR,
  });
  await manager.ensureRunning();
}

export async function teardown(): Promise<void> {
  await manager?.teardown();
}
