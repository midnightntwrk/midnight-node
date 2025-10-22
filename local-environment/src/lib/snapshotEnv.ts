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

export const SNAPSHOT_ENDPOINT_ENV = "MN_SNAPSHOT_S3_ENDPOINT_URL";
export const AWS_ACCESS_KEY_ENV = "AWS_ACCESS_KEY_ID";
export const AWS_SECRET_KEY_ENV = "AWS_SECRET_ACCESS_KEY";
export const AWS_SESSION_TOKEN_ENV = "AWS_SESSION_TOKEN";

export interface SnapshotCredentials {
  endpointUrl: string;
  accessKeyId: string;
  secretAccessKey: string;
  sessionToken?: string;
}

export function ensureSnapshotCredentials(
  env: Record<string, string>,
): SnapshotCredentials {
  const endpointUrl = env[SNAPSHOT_ENDPOINT_ENV]?.trim();
  if (!endpointUrl) {
    throw new Error(
      `Missing required snapshot S3 endpoint URL. Set ${SNAPSHOT_ENDPOINT_ENV}.`,
    );
  }

  const accessKeyId = env[AWS_ACCESS_KEY_ENV]?.trim();
  if (!accessKeyId) {
    throw new Error(
      `Missing required AWS access key. Set ${AWS_ACCESS_KEY_ENV}.`,
    );
  }

  const secretAccessKey = env[AWS_SECRET_KEY_ENV]?.trim();
  if (!secretAccessKey) {
    throw new Error(
      `Missing required AWS secret key. Set ${AWS_SECRET_KEY_ENV}.`,
    );
  }

  const sessionToken = env[AWS_SESSION_TOKEN_ENV]?.trim();

  return {
    endpointUrl,
    accessKeyId,
    secretAccessKey,
    sessionToken: sessionToken || undefined,
  };
}