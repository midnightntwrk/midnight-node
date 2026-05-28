// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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

import fs from "fs";
import path from "path";
import { spawnSync } from "child_process";

export interface MockAuthoritiesConvertOptions {
  /** Host path to the restored snapshot data dir (must contain chains/<chainId>/...). */
  dataDir: string;
  /** Host path where mock-registrations.json + seeds/ will be written. */
  outputDir: string;
  /** Substrate chain id; must match chains/<chainId> inside dataDir. */
  chainId: string;
  /** Number of validators to materialize. */
  numValidators: number;
  /** mock-authorities docker image (e.g. ghcr.io/shieldedtech/mock-authorities:0f347c5). */
  image: string;
}

const DEFAULT_IMAGE = "ghcr.io/shieldedtech/mock-authorities:0f347c5";

export function defaultMockAuthoritiesImage(): string {
  return process.env.MOCK_AUTHORITIES_IMAGE ?? DEFAULT_IMAGE;
}

/**
 * Runs `mock-authorities convert` against an extracted snapshot data dir,
 * writing mock-registrations.json + seeds/validator-N/{aura,grandpa,cross_chain}.seed
 * into outputDir. Mirrors fork-network.yml's `Run mock-authorities convert` step.
 */
export function runMockAuthoritiesConvert(
  opts: MockAuthoritiesConvertOptions,
): void {
  const dataDir = path.resolve(opts.dataDir);
  const outputDir = path.resolve(opts.outputDir);

  if (!fs.existsSync(dataDir)) {
    throw new Error(`mock-authorities: dataDir does not exist: ${dataDir}`);
  }

  // The mock-authorities container runs as a non-root appuser and needs to
  // write into outputDir. Pre-create with permissive perms so the bind mount
  // is writable regardless of the container uid.
  fs.mkdirSync(outputDir, { recursive: true });
  spawnSync("chmod", ["a+rwX", outputDir], { stdio: "inherit" });

  console.log(
    `Running mock-authorities convert (chain=${opts.chainId}, validators=${opts.numValidators})`,
  );
  const result = spawnSync(
    "docker",
    [
      "run",
      "--rm",
      "-v",
      `${dataDir}:/data`,
      "-v",
      `${outputDir}:/out`,
      opts.image,
      "convert",
      "--data-dir",
      "/data",
      "--chain-id",
      opts.chainId,
      "--num-validators",
      String(opts.numValidators),
      "--output-dir",
      "/out",
    ],
    { stdio: "inherit" },
  );

  if (result.status !== 0) {
    throw new Error(
      `mock-authorities convert failed (exit ${result.status}). image=${opts.image}`,
    );
  }

  const registrationsFile = path.join(outputDir, "mock-registrations.json");
  if (!fs.existsSync(registrationsFile)) {
    throw new Error(
      `mock-authorities convert produced no mock-registrations.json at ${registrationsFile}`,
    );
  }
  const seedsDir = path.join(outputDir, "seeds");
  if (!fs.existsSync(seedsDir)) {
    throw new Error(
      `mock-authorities convert produced no seeds/ directory at ${seedsDir}`,
    );
  }
}
