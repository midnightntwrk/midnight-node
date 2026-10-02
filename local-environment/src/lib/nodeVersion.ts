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

type VersionCore = [number, number, number];

/**
 * Parses the leading `major.minor.patch` of a node version string such as the
 * `system_version` RPC result (`3.0.0-1a2b3c4d`). Any suffix after the core is
 * ignored. Returns `undefined` when the string does not start with a version core.
 */
export function parseVersionCore(version: string): VersionCore | undefined {
  const match = /^v?(\d+)\.(\d+)\.(\d+)/.exec(version.trim());
  if (!match) {
    return undefined;
  }
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

/**
 * Whether `nodeVersion` is at least `requiredVersion`, comparing
 * `major.minor.patch` only. Returns `undefined` when either string has no
 * version core.
 */
export function isNodeVersionAtLeast(
  nodeVersion: string,
  requiredVersion: string,
): boolean | undefined {
  const actual = parseVersionCore(nodeVersion);
  const required = parseVersionCore(requiredVersion);
  if (!actual || !required) {
    return undefined;
  }
  for (let i = 0; i < 3; i++) {
    if (actual[i] !== required[i]) {
      return actual[i] > required[i];
    }
  }
  return true;
}
