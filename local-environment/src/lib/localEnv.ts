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

import fs from "fs";
import crypto from "crypto";
import dotenv from "dotenv";
import * as path from "path";

export const requiredImageVars = [
  "CARDANO_IMAGE",
  "DBSYNC_IMAGE",
  "OGMIOS_IMAGE",
  "INDEXER_CHAIN_IMAGE",
  "INDEXER_WALLET_IMAGE",
  "INDEXER_API_IMAGE",
  "KUPO_IMAGE",
  "YACI_STORE_IMAGE",
  "YACI_VIEWER_IMAGE",
  "ARCHITECTURE",
];

export const LOCAL_ENV_FILE_PATH = "../../.env.default";

export function generateSecretsIfMissing() {
  const secrets: [string, number][] = [
    ["localenv_postgres.password", 16],
    ["localenv_app_storage.password", 16],
    ["localenv_pubsub.password", 32],
    ["localenv_ledger_state_storage.password", 32],
    ["localenv_app_infra_secret.password", 32],
  ];

  for (const [filename, size] of secrets) {
    if (!fs.existsSync(filename)) {
      const buf = crypto.randomBytes(size);
      const content =
        size === 16 ? buf.toString("hex").slice(0, 16) : buf.toString("hex");
      console.log(`Writing secret ${filename}`);
      fs.writeFileSync(filename, content);
    }
  }
}

export function getLocalEnvSecretVars(): Record<string, string> {
  const result: Record<string, string> = {};

  if (!process.env.LOCALENV_POSTGRES_PASSWORD) {
    result.LOCALENV_POSTGRES_PASSWORD = fs
      .readFileSync("localenv_postgres.password", "utf-8")
      .trim();
  }

  if (!process.env.APP__INFRA__STORAGE__PASSWORD) {
    result.APP__INFRA__STORAGE__PASSWORD = fs
      .readFileSync("localenv_app_storage.password", "utf-8")
      .trim();
  }

  if (!process.env.APP__INFRA__PUB_SUB__PASSWORD) {
    result.APP__INFRA__PUB_SUB__PASSWORD = fs
      .readFileSync("localenv_pubsub.password", "utf-8")
      .trim();
  }

  if (!process.env.APP__INFRA__SECRET) {
    result.APP__INFRA__SECRET = fs
      .readFileSync("localenv_app_infra_secret.password", "utf-8")
      .trim();
  }

  return result;
}

export function loadEnvDefault(): Record<string, string> {
  const envPath = path.join(__dirname, LOCAL_ENV_FILE_PATH);

  if (!fs.existsSync(envPath)) {
    console.warn(`⚠️  No .env.default file found at: ${envPath}`);
    return {};
  }

  const parsed = dotenv.parse(fs.readFileSync(envPath));
  return parsed;
}
