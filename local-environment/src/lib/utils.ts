// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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
import path from "path";

export function isPathWithinBase(candidate: string, base: string): boolean {
  const normalized = path.resolve(candidate);
  const baseResolved = path.resolve(base);

  if (normalized === baseResolved) {
    return true;
  }

  return normalized.startsWith(`${baseResolved}${path.sep}`);
}

function realpathIfExists(candidate: string): string {
  if (fs.existsSync(candidate)) {
    return fs.realpathSync(candidate);
  }
  return candidate;
}

export function resolvePathWithinBase(
  base: string,
  label: string,
  ...parts: string[]
): string {
  const resolvedBase = realpathIfExists(path.resolve(base));
  const resolvedCandidate = realpathIfExists(
    path.resolve(resolvedBase, ...parts),
  );

  if (!isPathWithinBase(resolvedCandidate, resolvedBase)) {
    throw new Error(`${label} must resolve within ${resolvedBase}`);
  }

  return resolvedCandidate;
}

export function resolveInputPath(
  input: string,
  options: {
    baseDir?: string;
    label: string;
    allowAbsoluteOutsideBase?: boolean;
  },
): string {
  const trimmed = input?.trim();
  if (!trimmed) {
    throw new Error(`${options.label} is required and cannot be empty`);
  }

  const isAbsolute = path.isAbsolute(trimmed);
  const baseDir = options.baseDir ? path.resolve(options.baseDir) : undefined;
  const resolved = isAbsolute
    ? path.resolve(trimmed)
    : path.resolve(baseDir ?? process.cwd(), trimmed);

  const resolvedForCheck = realpathIfExists(resolved);

  if (baseDir) {
    const baseForCheck = realpathIfExists(baseDir);
    const enforceBase = !(options.allowAbsoluteOutsideBase && isAbsolute);
    if (enforceBase && !isPathWithinBase(resolvedForCheck, baseForCheck)) {
      throw new Error(`${options.label} must resolve within ${baseForCheck}`);
    }
  }

  return resolvedForCheck;
}
