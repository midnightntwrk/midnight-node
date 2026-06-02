// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { imageUpgrade } from "./imageUpgrade";
import { federatedRuntimeUpgrade } from "./federatedRuntimeUpgrade";
import { FullUpgradeOptions } from "../lib/types";

/**
 * Two-phase upgrade rehearsal mirroring the live rollout:
 *   1. Roll the validator client image to the new tag (still on the existing runtime).
 *   2. Submit the governance-approved runtime upgrade against the running set.
 *
 * Phase 1 handles snapshot restore and bring-up if --from-snapshot is given;
 * phase 2 is invoked with skipRun=true and no fromSnapshot so it never re-restores
 * or re-ups the environment.
 */
export async function fullUpgrade(
  namespace: string,
  opts: FullUpgradeOptions,
): Promise<void> {
  console.log(`[full-upgrade ${namespace}] phase 1/2: client image rollout`);
  await imageUpgrade(namespace, {
    profiles: opts.profiles,
    envFile: opts.envFile,
    fromSnapshot: opts.fromSnapshot,
    imageEnvVar: opts.imageEnvVar,
    services: opts.services,
    includePattern: opts.includePattern,
    excludePattern: opts.excludePattern,
    waitBeforeMs: opts.waitBeforeMs,
    waitBetweenMs: opts.waitBetweenMs,
    healthTimeoutSec: opts.healthTimeoutSec,
    requireHealthy: opts.requireHealthy,
  });

  console.log(
    `[full-upgrade ${namespace}] phase 2/2: governance runtime upgrade`,
  );
  await federatedRuntimeUpgrade(namespace, {
    profiles: opts.profiles,
    envFile: opts.envFile,
    fromSnapshot: undefined,
    skipRun: true,
    wasmPath: opts.wasmPath,
    rpcUrl: opts.rpcUrl,
    councilUris: opts.councilUris,
    techCommitteeUris: opts.techCommitteeUris,
    motionExecutorUri: opts.motionExecutorUri,
    allowSameVersion: opts.allowSameVersion,
  });

  console.log(`[full-upgrade ${namespace}] complete`);
}
