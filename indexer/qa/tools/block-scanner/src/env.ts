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

const INDEXER_BASE_URL: Record<string, string> = {
  undeployed: "localhost:8088",
  devnet: "indexer.devnet.midnight.network",
  preview: "indexer.preview.midnight.network",
  preprod: "indexer.preprod.midnight.network",
  qanet: "indexer.qanet.midnight.network",
  stagenet: "indexer.stagenet.shielded.tools",
};

export let TARGET_ENV: string;
export let INDEXER_API_VERSION: string;

if (Bun.env.TARGET_ENV === undefined || Bun.env.TARGET_ENV === "") {
  console.warn(
    "[WARN ] - TARGET_ENV not set, default to undeployed environment",
  );
  TARGET_ENV = "undeployed";
} else {
  TARGET_ENV = Bun.env.TARGET_ENV;
  console.info(`[INFO ] - Target environment: ${TARGET_ENV}`);
}

if (
  Bun.env.INDEXER_API_VERSION === undefined ||
  Bun.env.INDEXER_API_VERSION === ""
) {
  console.warn(
    "[WARN ] - INDEXER_API_VERSION not set explicitly, default to v4",
  );
  INDEXER_API_VERSION = "v4";
} else {
  INDEXER_API_VERSION = Bun.env.INDEXER_API_VERSION;
  console.info(`[INFO ] - Indexer API version: ${INDEXER_API_VERSION}`);
}

export const INDEXER_WS_URL: string =
  TARGET_ENV === "undeployed"
    ? `ws://${INDEXER_BASE_URL[TARGET_ENV]}/api/${INDEXER_API_VERSION}/graphql/ws`
    : `wss://${INDEXER_BASE_URL[TARGET_ENV]}/api/${INDEXER_API_VERSION}/graphql/ws`;

export const INDEXER_HTTP_URL: string =
  TARGET_ENV === "undeployed"
    ? `http://${INDEXER_BASE_URL[TARGET_ENV]}`
    : `https://${INDEXER_BASE_URL[TARGET_ENV]}`;
