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

import fs from "fs";
import { parse } from "dotenv";

export function cleanEnv(
  env: Record<string, string | undefined>,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(env).filter(([, v]) => typeof v === "string"),
  ) as Record<string, string>;
}

/** Merges each `--env-file`'s vars onto `base`, in order; later files win. */
export function applyEnvFileOverrides(
  base: Record<string, string>,
  envFile: string[] | undefined,
): Record<string, string> {
  let env = { ...base };
  for (const envFilePath of envFile ?? []) {
    if (fs.existsSync(envFilePath)) {
      env = { ...env, ...parse(fs.readFileSync(envFilePath)) };
    } else {
      console.warn(`⚠️  Env file not found: ${envFilePath}`);
    }
  }
  return env;
}
