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

import { imageUpgrade } from "./imageUpgrade";
import { runtimeUpgrade } from "./runtimeUpgrade";
import { SequentialUpgradeOptions } from "../lib/types";

/**
 * Run a runtime upgrade followed by a client image rollout while sharing
 * snapshot state between the two steps.
 */
export async function runtimeThenImage(
  network: string,
  options: SequentialUpgradeOptions,
) {
  console.log(
    `Executing upgrade sequence for ${network}: runtime upgrade → client rollout`,
  );

  const runtimeWithSnapshot = {
    ...options.runtime,
    fromSnapshot: options.fromSnapshot ?? options.runtime.fromSnapshot,
  };

  await runtimeUpgrade(network, runtimeWithSnapshot);

  const imageWithoutSnapshot = {
    ...options.image,
    fromSnapshot: undefined,
  };

  await imageUpgrade(network, imageWithoutSnapshot);
}

/**
 * Run a client image rollout followed by a runtime upgrade while reusing the
 * snapshot restored during the first step.
 */
export async function imageThenRuntime(
  network: string,
  options: SequentialUpgradeOptions,
) {
  console.log(
    `Executing upgrade sequence for ${network}: client rollout → runtime upgrade`,
  );

  const imageWithSnapshot = {
    ...options.image,
    fromSnapshot: options.fromSnapshot ?? options.image.fromSnapshot,
  };

  await imageUpgrade(network, imageWithSnapshot);

  const runtimeAfterClient = {
    ...options.runtime,
    fromSnapshot: undefined,
    skipRun: true,
  };

  await runtimeUpgrade(network, runtimeAfterClient);
}