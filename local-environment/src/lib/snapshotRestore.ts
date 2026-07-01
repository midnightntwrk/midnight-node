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
import os from "os";
import path from "path";
import { spawnSync } from "child_process";
import YAML from "yaml";

interface RestoreSnapshotOptions {
  namespace: string;
  composeFile: string;
  /** Fully-qualified http:// or https:// snapshot URI. */
  snapshotUri: string;
  env: Record<string, string>;
  /**
   * If true, `chmod -R a+rwX` each restored data dir after extraction so the
   * non-root container appuser can open parity-db RDWR.
   */
  permissive?: boolean;
  /**
   * Number of leading path components to strip when extracting the tar.
   * Snapshots from the backup system are wrapped in a top-level `node/` dir;
   * passing 1 strips it so chains/ lands at the data dir root.
   */
  stripComponents?: number;
}

export async function restoreSnapshot(
  options: RestoreSnapshotOptions,
): Promise<string[]> {
  const {
    namespace,
    composeFile,
    snapshotUri,
    env,
    permissive,
    stripComponents,
  } = options;
  if (!snapshotUri) return [];

  if (
    !snapshotUri.startsWith("http://") &&
    !snapshotUri.startsWith("https://")
  ) {
    throw new Error(
      `Unsupported snapshot URI scheme: ${snapshotUri}. Expected http:// or https://.`,
    );
  }

  const resolvedCompose = path.resolve(composeFile);
  const composeDir = path.dirname(resolvedCompose);
  const dataRoot = path.resolve(composeDir, "data");

  console.log(
    `Restoring snapshot '${snapshotUri}' for namespace '${namespace}'`,
  );

  const stagingDir = fs.mkdtempSync(
    path.join(os.tmpdir(), "midnight-snapshot-"),
  );
  const archiveName = determineArchiveName(snapshotUri);
  const archivePath = path.join(stagingDir, archiveName);
  const extractDir = path.join(stagingDir, "extracted");
  fs.mkdirSync(extractDir, { recursive: true });

  const mergedEnv: NodeJS.ProcessEnv = { ...process.env, ...env };

  try {
    downloadSnapshot(snapshotUri, archivePath, mergedEnv);

    const unpackedRoot = extractSnapshotArchive(
      archivePath,
      extractDir,
      stripComponents,
    );

    const targetDirs = discoverDataMounts(resolvedCompose, dataRoot);
    if (targetDirs.length === 0) {
      console.warn(
        `No data directories discovered in compose file ${resolvedCompose}; skipping snapshot restore`,
      );
      return [];
    }

    replicateSnapshot(unpackedRoot, targetDirs);

    if (permissive) {
      makeWritableForContainerUser(targetDirs);
    }

    const noun = targetDirs.length === 1 ? "directory" : "directories";
    console.log(`Restored snapshot into ${targetDirs.length} data ${noun}`);
    return targetDirs;
  } finally {
    fs.rmSync(stagingDir, { recursive: true, force: true });
  }
}

export function discoverComposeDataMounts(composeFile: string): string[] {
  const resolvedCompose = path.resolve(composeFile);
  const composeDir = path.dirname(resolvedCompose);
  const dataRoot = path.resolve(composeDir, "data");

  return discoverDataMounts(resolvedCompose, dataRoot);
}

function ensureBinary(bin: string, hint: string, env: NodeJS.ProcessEnv) {
  const check = spawnSync(bin, ["--version"], { stdio: "ignore", env });
  if (check.status !== 0) {
    throw new Error(`${bin} is required to restore snapshots. ${hint}`);
  }
}

function downloadSnapshot(
  snapshotUri: string,
  destination: string,
  env: NodeJS.ProcessEnv,
) {
  console.log(`Downloading snapshot archive to ${destination}`);
  ensureBinary("curl", "Install curl or ensure it is on PATH.", env);
  const result = spawnSync(
    "curl",
    [
      "-fL",
      "--retry",
      "3",
      "--retry-delay",
      "5",
      "-o",
      destination,
      snapshotUri,
    ],
    { stdio: "inherit", env },
  );
  if (result.status !== 0) {
    throw new Error(`Failed to download snapshot from ${snapshotUri}`);
  }
}

function makeWritableForContainerUser(targetDirs: string[]): void {
  for (const dir of targetDirs) {
    const result = spawnSync("chmod", ["-R", "a+rwX", dir], {
      stdio: "inherit",
    });
    if (result.status !== 0) {
      throw new Error(`Failed to chmod restored data dir: ${dir}`);
    }
  }
}

function extractSnapshotArchive(
  archivePath: string,
  dest: string,
  stripComponents?: number,
): string {
  if (!fs.existsSync(archivePath)) {
    throw new Error(`Snapshot archive missing at ${archivePath}`);
  }

  if (archivePath.endsWith(".tar.zst") || archivePath.endsWith(".zst")) {
    const tarPath = decompressZstd(archivePath);
    return untarArchive(tarPath, dest, stripComponents);
  }

  if (
    archivePath.endsWith(".tar.gz") ||
    archivePath.endsWith(".tgz") ||
    archivePath.endsWith(".tar")
  ) {
    return untarArchive(archivePath, dest, stripComponents);
  }

  throw new Error(
    `Unsupported snapshot archive format for ${archivePath}. Expected .tar.zst, .tar.gz, or .tar`,
  );
}

function decompressZstd(archivePath: string): string {
  const check = spawnSync("zstd", ["-V"], { stdio: "ignore" });
  if (check.status !== 0) {
    throw new Error(
      "zstd binary is required to decompress .zst archives. Install zstd and retry.",
    );
  }

  console.log(`Decompressing ${archivePath} with zstd`);
  const result = spawnSync("zstd", ["-d", "--force", archivePath], {
    stdio: "inherit",
  });
  if (result.status !== 0) {
    throw new Error(`Failed to decompress ${archivePath} with zstd`);
  }

  const tarPath = archivePath.replace(/\.zst$/, "");
  if (!fs.existsSync(tarPath)) {
    throw new Error(`Expected tar file ${tarPath} after zstd decompression`);
  }

  return tarPath;
}

function untarArchive(
  archivePath: string,
  dest: string,
  stripComponents?: number,
): string {
  console.log(`Extracting ${archivePath} into ${dest}`);
  const baseFlags = archivePath.endsWith(".gz")
    ? ["-xzf", archivePath]
    : ["-xf", archivePath];
  const stripFlag =
    stripComponents && stripComponents > 0
      ? [`--strip-components=${stripComponents}`]
      : [];
  const args = [...baseFlags, "-C", dest, ...stripFlag];

  const result = spawnSync("tar", args, { stdio: "inherit" });
  if (result.status !== 0) {
    throw new Error(`Failed to extract archive ${archivePath}`);
  }

  return dest;
}

function discoverDataMounts(composeFile: string, dataRoot: string): string[] {
  const raw = fs.readFileSync(composeFile, "utf-8");
  const parsed = YAML.parse(raw);
  const composeDir = path.dirname(composeFile);
  const resolvedDataRoot = path.resolve(dataRoot);
  const mountPaths = new Set<string>();

  const services = parsed?.services;
  if (!services || typeof services !== "object") {
    return [];
  }

  for (const service of Object.values(services) as unknown[]) {
    if (!service || typeof service !== "object") continue;
    const volumes = (service as Record<string, unknown>).volumes;
    if (!Array.isArray(volumes)) continue;

    for (const volume of volumes) {
      const hostPath = resolveVolumeSource(volume);
      if (!hostPath) continue;

      const resolved = path.resolve(composeDir, hostPath);
      if (isUnderDataRoot(resolved, resolvedDataRoot)) {
        mountPaths.add(resolved);
      }
    }
  }

  return Array.from(mountPaths).sort();
}

function resolveVolumeSource(volume: unknown): string | undefined {
  if (typeof volume === "string") {
    const [hostPart] = volume.split(":", 1);
    return hostPart?.trim();
  }

  if (volume && typeof volume === "object") {
    const source = (volume as Record<string, unknown>).source;
    if (typeof source === "string") {
      return source.trim();
    }
  }

  return undefined;
}

function isUnderDataRoot(candidate: string, dataRoot: string): boolean {
  const normalized = path.resolve(candidate);
  const base = path.resolve(dataRoot);

  if (normalized === base) {
    return true;
  }

  return normalized.startsWith(`${base}${path.sep}`);
}

function replicateSnapshot(sourceDir: string, targets: string[]): void {
  const entries = fs.readdirSync(sourceDir);
  for (const target of targets) {
    fs.rmSync(target, { recursive: true, force: true });
    fs.mkdirSync(target, { recursive: true });

    for (const entry of entries) {
      const src = path.join(sourceDir, entry);
      const dest = path.join(target, entry);
      fs.cpSync(src, dest, { recursive: true });
    }
  }
}

function determineArchiveName(snapshotUri: string): string {
  const parts = snapshotUri.split("/");
  const last = parts[parts.length - 1];
  if (last && last.length > 0) {
    const sanitized = last.split("?")[0]?.trim();
    if (sanitized) {
      return sanitized;
    }
  }
  return "snapshot.tar";
}
