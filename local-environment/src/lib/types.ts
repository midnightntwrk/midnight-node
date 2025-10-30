// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Structure of any extra run options for the CLI
export interface RunOptions {
  profiles?: string[];
  envFile?: string[];
  fromSnapshot?: string;
}

export interface ImageUpgradeOptions extends RunOptions {
  /** which env var controls the node image tag in your compose files default: MN_IMAGE_TAG */
  imageEnvVar?: string;
  /** explicit list of services to roll; if omitted, we'll infer via `docker compose config --services` */
  services?: string[];
  /** regex to include services (applied after explicit list if provided) */
  includePattern?: string;
  /** regex to exclude services */
  excludePattern?: string;
  /** time (ms) to wait between each service rollout default 5000 */
  waitBetweenMs?: number;
  /** max seconds to wait for a service to report healthy after restart default 180 */
  healthTimeoutSec?: number;
  /** if true, require container health=healthy; otherwise we only waitBetweenMs default true */
  requireHealthy?: boolean;
}

export interface RuntimeUpgradeOptions extends RunOptions {
  /** absolute or relative path to the runtime wasm artifact */
  wasmPath: string;
  /** sudo key URI used to submit the upgrade (defaults to env/"//Alice") */
  sudoUri?: string;
  /** how many blocks to wait before submitting the sudo upgrade */
  delayBlocks?: number;
  /** skip bringing up docker-compose before submitting the upgrade */
  skipRun?: boolean;
  /** websocket endpoint for the node under upgrade (default ws://localhost:9944) */
  rpcUrl?: string;
}

export interface SnapshotOptions {
  /** name of the bootnode statefulset to snapshot */
  bootnodeStatefulSet?: string;
  /** optional pvc name override */
  pvcName?: string;
  /** s3 uri that receives the archive */
  s3Uri?: string;
  /** container image used to perform the snapshot */
  snapshotImage?: string;
  /** timeout window in minutes */
  timeoutMinutes?: number;
}
