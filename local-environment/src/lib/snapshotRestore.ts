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
import os from "os";
import path from "path";
import { spawnSync } from "child_process";
import YAML from "yaml";
import { ensureSnapshotCredentials } from "./snapshotEnv";

interface RestoreSnapshotOptions {
  namespace: string;
  composeFile: string;
  snapshotId: string;
  env: Record<string, string>;
}

export async function restoreSnapshotFromS3(
  options: RestoreSnapshotOptions,
): Promise<void> {
  const { namespace, composeFile, snapshotId, env } = options;
  if (!snapshotId) return;

  const resolvedCompose = path.resolve(composeFile);
  const composeDir = path.dirname(resolvedCompose);
  const dataRoot = path.resolve(composeDir, "data");

  const snapshotUri = resolveSnapshotUri(snapshotId, env);

  console.log(
    `Restoring snapshot '${snapshotUri}' for namespace '${namespace}'`,
  );

  ensureAwsCli(env);

  const stagingDir = fs.mkdtempSync(
    path.join(os.tmpdir(), "midnight-snapshot-"),
  );
  const archiveName = determineArchiveName(snapshotUri);
  const archivePath = path.join(stagingDir, archiveName);
  const extractDir = path.join(stagingDir, "extracted");
  fs.mkdirSync(extractDir, { recursive: true });

  const mergedEnv: NodeJS.ProcessEnv = {
    ...process.env,
    ...env,
  };

  try {
    const credentials = ensureSnapshotCredentials(toStringRecord(mergedEnv));
    downloadSnapshot(
      snapshotUri,
      archivePath,
      mergedEnv,
      credentials.endpointUrl,
    );

    const unpackedRoot = extractSnapshotArchive(archivePath, extractDir);

    const targetDirs = discoverDataMounts(resolvedCompose, dataRoot);
    if (targetDirs.length === 0) {
      console.warn(
        `No data directories discovered in compose file ${resolvedCompose}; skipping snapshot restore`,
      );
      return;
    }

    replicateSnapshot(unpackedRoot, targetDirs);

    const noun = targetDirs.length === 1 ? "directory" : "directories";
    console.log(`Restored snapshot into ${targetDirs.length} data ${noun}`);
  } finally {
    fs.rmSync(stagingDir, { recursive: true, force: true });
  }
}

function resolveSnapshotUri(
  snapshotId: string,
  env: Record<string, string>,
): string {
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(snapshotId)) {
    return snapshotId;
  }

  const base = env.MN_SNAPSHOT_S3_URI ?? process.env.MN_SNAPSHOT_S3_URI;
  if (!base || base.trim().length === 0) {
    throw new Error(
      "No snapshot S3 base URI provided. Pass --from-snapshot with a fully qualified URI or set MN_SNAPSHOT_S3_URI.",
    );
  }

  const root = base.replace(/\/$/, "");

  const trimmedId = snapshotId.replace(/^\/+/, "");
  return `${root}/${trimmedId}`;
}

function ensureAwsCli(env: Record<string, string>) {
  const check = spawnSync("aws", ["--version"], {
    stdio: "ignore",
    env: { ...process.env, ...env },
  });

  if (check.status !== 0) {
    throw new Error(
      "AWS CLI is required to restore snapshots. Install it locally or ensure it is on your PATH.",
    );
  }
}

function downloadSnapshot(
  snapshotUri: string,
  destination: string,
  env: NodeJS.ProcessEnv,
  endpointUrl: string,
) {
  console.log(`Downloading snapshot archive to ${destination}`);
  const result = spawnSync(
    "aws",
    ["s3", "cp", "--endpoint-url", endpointUrl, snapshotUri, destination],
    {
      stdio: "inherit",
      env,
    },
  );

  if (result.status !== 0) {
    throw new Error(`Failed to download snapshot from ${snapshotUri}`);
  }
}

function extractSnapshotArchive(archivePath: string, dest: string): string {
  if (!fs.existsSync(archivePath)) {
    throw new Error(`Snapshot archive missing at ${archivePath}`);
  }

  if (archivePath.endsWith(".tar.zst") || archivePath.endsWith(".zst")) {
    const tarPath = decompressZstd(archivePath);
    return untarArchive(tarPath, dest);
  }

  if (
    archivePath.endsWith(".tar.gz") ||
    archivePath.endsWith(".tgz") ||
    archivePath.endsWith(".tar")
  ) {
    return untarArchive(archivePath, dest);
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

function untarArchive(archivePath: string, dest: string): string {
  console.log(`Extracting ${archivePath} into ${dest}`);
  const args = archivePath.endsWith(".gz")
    ? ["-xzf", archivePath, "-C", dest]
    : ["-xf", archivePath, "-C", dest];

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
      const destBase = path.join(target, entry);

      // Hack for networks with unexpected names due to misconfiguration(ie devnet/qanet)
      if (entry === "chains") {
        const chainChildren = fs.readdirSync(src);
        for (const child of chainChildren) {
          const srcChild = path.join(src, child);
          if (child === "devnet") {
            const qanetDest = path.join(destBase, "qanet");
            fs.cpSync(srcChild, qanetDest, { recursive: true });
          }
        }
      }

      const dest = path.join(target, entry);
      fs.cpSync(src, dest, { recursive: true });
    }

    const keystorePath = path.join(target, "keystore");
    if (fs.existsSync(keystorePath)) {
      fs.rmSync(keystorePath, { recursive: true, force: true });
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

function toStringRecord(env: NodeJS.ProcessEnv): Record<string, string> {
  return Object.fromEntries(
    Object.entries(env).filter(([, value]) => typeof value === "string"),
  ) as Record<string, string>;
}