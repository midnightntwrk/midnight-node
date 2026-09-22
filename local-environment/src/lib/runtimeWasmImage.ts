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

import { execFileSync } from "child_process";
import fs from "fs";
import path from "path";

/**
 * Pull the runtime wasm out of a node image instead of requiring a pre-populated
 * `local-environment/artifacts/`.
 *
 * Every node image ships the runtime it was built with under `/artifacts-<arch>/`
 * (see `node-image` in the Earthfile), so the image itself is the authoritative
 * source for "the runtime that goes with this client". Extraction uses
 * `docker create` + `docker cp` rather than `docker run`, so nothing in the image
 * is executed and the image's entrypoint/user are irrelevant.
 *
 * The blob is written under the `artifacts/` directory because that is the only
 * root `loadRuntimeWasm` accepts — the sandbox stays in force, this just fills it.
 */

export const RUNTIME_WASM_FILENAME = "midnight_node_runtime.compact.compressed.wasm";

/** Subdirectory of `artifacts/` that image-extracted runtimes land in. */
export const IMAGE_WASM_SUBDIR = "from-image";

/** Architectures tried when the image's own architecture does not yield a hit. */
const FALLBACK_ARCHITECTURES = ["amd64", "arm64"] as const;

/**
 * Runs `docker` with the given args and returns stdout. Injectable so the
 * extraction logic is testable without a daemon.
 */
export type DockerExec = (args: string[]) => string;

export const defaultDockerExec: DockerExec = (args) =>
  execFileSync("docker", args, {
    encoding: "utf-8",
    stdio: ["ignore", "pipe", "pipe"],
  });

export interface ExtractRuntimeWasmOptions {
  /** Image reference to extract from, e.g. ghcr.io/midnight-ntwrk/midnight-node:latest-amd64 */
  image: string;
  /** Root the wasm is written under; defaults to `<cwd>/artifacts`. */
  artifactsRoot?: string;
  docker?: DockerExec;
}

export interface ExtractedRuntimeWasm {
  /** Path relative to the artifacts root — what `loadRuntimeWasm` expects. */
  relPath: string;
  /** Absolute path of the extracted blob. */
  absPath: string;
  /** Architecture directory it came from. */
  architecture: string;
}

export function artifactsRootPath(): string {
  return path.resolve(process.cwd(), "artifacts");
}

/**
 * Turn an image reference into a single filesystem-safe path segment. Image refs
 * carry `/`, `:` and `@`, none of which may leak into a path we then join.
 */
export function sanitizeImageRef(image: string): string {
  const trimmed = image?.trim();
  if (!trimmed) {
    throw new Error("Image reference is required and cannot be empty");
  }
  const safe = trimmed.replace(/[^A-Za-z0-9._-]+/g, "_").replace(/^[._]+/, "");
  if (!safe || safe === "." || safe === "..") {
    throw new Error(`Image reference '${image}' has no usable characters`);
  }
  return safe;
}

/**
 * The image a runtime upgrade should take its wasm from when the caller did not
 * name one: the image an upgrade is rolling *to* if there is one, else the image
 * the network is running.
 */
export function resolveWasmSourceImage(
  env: NodeJS.ProcessEnv = process.env,
): string | undefined {
  for (const key of ["NEW_NODE_IMAGE", "NODE_IMAGE", "MIDNIGHT_NODE_IMAGE"]) {
    const value = env[key]?.trim();
    if (value) return value;
  }
  return undefined;
}

/**
 * True when the wasm source is the image the network is already running, which
 * means the candidate shares its spec_version with the live runtime.
 */
export function isRunningImage(
  image: string,
  env: NodeJS.ProcessEnv = process.env,
): boolean {
  const newImage = env.NEW_NODE_IMAGE?.trim();
  if (newImage && newImage === image) return false;
  return [env.NODE_IMAGE?.trim(), env.MIDNIGHT_NODE_IMAGE?.trim()].includes(
    image,
  );
}

function imageArchitecture(image: string, docker: DockerExec): string[] {
  let detected: string | undefined;
  try {
    detected = docker([
      "image",
      "inspect",
      "--format",
      "{{.Architecture}}",
      image,
    ])
      .trim()
      .split("\n")[0]
      ?.trim();
  } catch {
    // Not pulled yet, or a manifest list with no local variant: fall through to
    // the fallbacks, which is also what the CI extraction step does.
  }
  const candidates = [detected, ...FALLBACK_ARCHITECTURES].filter(
    (a): a is string => Boolean(a),
  );
  return [...new Set(candidates)];
}

/**
 * Make sure `image` is available and up to date: pull it so a moved tag isn't
 * served stale, but if the pull fails (no such repository, offline, ...) fall
 * back to a local copy rather than pulling. Only fails if the image is in
 * neither place.
 */
function ensureImageAvailable(image: string, docker: DockerExec): void {
  try {
    docker(["pull", image]);
    return;
  } catch (pullErr) {
    try {
      docker(["image", "inspect", image]);
    } catch {
      throw new Error(
        `Image '${image}' was not found locally or in a registry: ${(pullErr as Error).message.trim()}`,
      );
    }
    console.warn(`⚠️  Could not pull ${image}; using the local copy.`);
  }
}

export function selectRuntimeVariant(dir: string): string | undefined {
  return fs.existsSync(path.join(dir, RUNTIME_WASM_FILENAME))
    ? RUNTIME_WASM_FILENAME
    : undefined;
}

/**
 * Copy `/artifacts-<arch>/` out of `image` and return the preferred runtime wasm
 * in it. Re-extracts every call and pulls before creating the container:
 * image tags such as `latest` move, and a stale local image or blob would
 * silently upgrade the wrong runtime. Falls back to the local image if the
 * pull fails, so a locally-built image never pushed to a registry still works.
 */
export function extractRuntimeWasmFromImage(
  opts: ExtractRuntimeWasmOptions,
): ExtractedRuntimeWasm {
  const docker = opts.docker ?? defaultDockerExec;
  const image = opts.image?.trim();
  if (!image) {
    throw new Error("Image reference is required and cannot be empty");
  }

  const root = opts.artifactsRoot ?? artifactsRootPath();
  const destDir = path.join(root, IMAGE_WASM_SUBDIR, sanitizeImageRef(image));
  fs.rmSync(destDir, { recursive: true, force: true });
  fs.mkdirSync(destDir, { recursive: true });

  const architectures = imageArchitecture(image, docker);
  ensureImageAvailable(image, docker);
  const containerId = docker(["create", image])
    .trim()
    .split("\n")
    .pop()
    ?.trim();
  if (!containerId) {
    throw new Error(`docker create ${image} returned no container id`);
  }

  const failures: string[] = [];
  try {
    for (const arch of architectures) {
      try {
        docker(["cp", `${containerId}:/artifacts-${arch}/.`, destDir]);
      } catch (err) {
        failures.push(`/artifacts-${arch}: ${(err as Error).message.trim()}`);
        continue;
      }
      const variant = selectRuntimeVariant(destDir);
      if (!variant) {
        failures.push(`/artifacts-${arch}: no ${RUNTIME_WASM_FILENAME}`);
        continue;
      }
      const absPath = path.join(destDir, variant);
      return {
        relPath: path.relative(root, absPath),
        absPath,
        architecture: arch,
      };
    }
  } finally {
    try {
      docker(["rm", "-f", containerId]);
    } catch {
      // best-effort cleanup; a leaked created (never started) container is harmless
    }
  }

  throw new Error(
    `No runtime wasm found in ${image}. Node images ship it under /artifacts-<arch>/ ` +
      `(see node-image in the Earthfile); tried ${architectures
        .map((a) => `/artifacts-${a}`)
        .join(", ")}.\n  ${failures.join("\n  ")}`,
  );
}

/**
 * Rejects `--wasm` combined with `--wasm-from-image`. Exported so callers that
 * do other (possibly expensive) work before resolving the wasm path, such as
 * `full-upgrade`'s image rollout phase, can fail on this before starting it.
 */
export function assertWasmSourceFlagsExclusive(opts: {
  wasmPath?: string;
  wasmFromImage?: string;
}): void {
  if (opts.wasmPath?.trim() && opts.wasmFromImage?.trim()) {
    throw new Error(
      "--wasm and --wasm-from-image are mutually exclusive; pass only one.",
    );
  }
}

/**
 * Resolve the runtime wasm for an upgrade: an explicit `artifacts/`-relative path,
 * or a blob extracted from a node image. Returns a path `loadRuntimeWasm` accepts.
 */
export function resolveRuntimeWasmPath(opts: {
  wasmPath?: string;
  wasmFromImage?: string;
  artifactsRoot?: string;
  docker?: DockerExec;
  env?: NodeJS.ProcessEnv;
}): string {
  assertWasmSourceFlagsExclusive(opts);
  const env = opts.env ?? process.env;
  const explicitPath = opts.wasmPath?.trim();
  const explicitImage = opts.wasmFromImage?.trim();

  if (explicitPath) return explicitPath;

  const image = explicitImage ?? resolveWasmSourceImage(env);
  if (!image) {
    throw new Error(
      "No runtime wasm source. Pass --wasm <path under artifacts/>, or " +
        "--wasm-from-image <image>, or set NEW_NODE_IMAGE / MIDNIGHT_NODE_IMAGE " +
        "so the runtime can be taken from the node image.",
    );
  }

  console.log(`Extracting runtime wasm from image ${image}`);
  const extracted = extractRuntimeWasmFromImage({
    image,
    artifactsRoot: opts.artifactsRoot,
    docker: opts.docker,
  });
  console.log(
    `Extracted ${extracted.relPath} from ${image} (/artifacts-${extracted.architecture})`,
  );

  if (isRunningImage(image, env)) {
    console.warn(
      `⚠️  ${image} is the image this network is already running, so the candidate ` +
        `runtime shares its spec_version. The upgrade will be rejected unless it bumps ` +
        `spec_version — pass --allow-same-version for a local rehearsal, or point ` +
        `--wasm-from-image at the image you are upgrading to.`,
    );
  }

  return extracted.relPath;
}
