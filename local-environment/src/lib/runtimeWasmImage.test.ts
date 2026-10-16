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

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, it } from "node:test";

import {
  DockerExec,
  IMAGE_WASM_SUBDIR,
  extractRuntimeWasmFromImage,
  isRunningImage,
  resolveRuntimeWasmPath,
  resolveWasmSourceImage,
  sanitizeImageRef,
  selectRuntimeVariant,
} from "./runtimeWasmImage";

const tmpDirs: string[] = [];

afterEach(() => {
  for (const dir of tmpDirs.splice(0)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

function artifactsRoot(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "wasm-image-"));
  tmpDirs.push(dir);
  return dir;
}

/**
 * Stand-in for the docker CLI. `layout` maps an arch directory to the files a
 * `docker cp` of it would produce; anything absent makes that cp fail the way
 * docker does for a missing path.
 */
function fakeDocker(opts: {
  architecture?: string;
  inspectFails?: boolean;
  pullFails?: boolean;
  layout: Record<string, string[]>;
  calls?: string[][];
}): DockerExec {
  return (args: string[]) => {
    opts.calls?.push(args);
    if (args[0] === "image" && args[1] === "inspect") {
      if (opts.inspectFails) throw new Error("No such image");
      return `${opts.architecture ?? "amd64"}\n`;
    }
    if (args[0] === "pull") {
      if (opts.pullFails) throw new Error("no such image on registry");
      return "";
    }
    if (args[0] === "create") return "container-id\n";
    if (args[0] === "rm") return "";
    if (args[0] === "cp") {
      const match = /:\/artifacts-([^/]+)\/\.$/.exec(args[1]);
      const files = match ? opts.layout[match[1]] : undefined;
      if (!files) {
        throw new Error(`Could not find the file ${args[1]} in container`);
      }
      for (const file of files) {
        fs.writeFileSync(path.join(args[2], file), "wasm");
      }
      return "";
    }
    throw new Error(`unexpected docker invocation: ${args.join(" ")}`);
  };
}

const IMAGE_ARTIFACTS_FILE_NAMES = [
  "midnight_node_runtime.wasm",
  "midnight_node_runtime.compact.wasm",
  "midnight_node_runtime.compact.compressed.wasm",
  "node_image_tag",
];

describe("sanitizeImageRef", () => {
  it("reduces a registry reference to one path segment", () => {
    assert.equal(
      sanitizeImageRef("ghcr.io/midnight-ntwrk/midnight-node:3.0.0-abc-amd64"),
      "ghcr.io_midnight-ntwrk_midnight-node_3.0.0-abc-amd64",
    );
  });

  it("keeps digests and tags from escaping the artifacts directory", () => {
    const safe = sanitizeImageRef("../../etc/passwd:tag");
    assert.ok(!safe.includes("/"));
    assert.ok(!safe.startsWith("."));
  });

  it("rejects an empty reference", () => {
    assert.throws(() => sanitizeImageRef("   "), /cannot be empty/);
  });
});

describe("resolveWasmSourceImage", () => {
  it("prefers the image an upgrade rolls to", () => {
    assert.equal(
      resolveWasmSourceImage({
        NEW_NODE_IMAGE: "node:new",
        NODE_IMAGE: "node:old",
      }),
      "node:new",
    );
  });

  it("falls back to the running image", () => {
    assert.equal(
      resolveWasmSourceImage({ MIDNIGHT_NODE_IMAGE: "node:running" }),
      "node:running",
    );
  });

  it("is undefined when no image env var is set", () => {
    assert.equal(resolveWasmSourceImage({}), undefined);
  });
});

describe("isRunningImage", () => {
  it("is true when the source is the image already running", () => {
    assert.equal(
      isRunningImage("node:a", { MIDNIGHT_NODE_IMAGE: "node:a" }),
      true,
    );
  });

  it("is false when the source is the upgrade target", () => {
    assert.equal(
      isRunningImage("node:b", {
        NODE_IMAGE: "node:a",
        NEW_NODE_IMAGE: "node:b",
      }),
      false,
    );
  });
});

describe("selectRuntimeVariant", () => {
  it("finds the compact compressed blob", () => {
    const dir = artifactsRoot();
    for (const f of IMAGE_ARTIFACTS_FILE_NAMES) fs.writeFileSync(path.join(dir, f), "wasm");
    assert.equal(
      selectRuntimeVariant(dir),
      "midnight_node_runtime.compact.compressed.wasm",
    );
  });

  it("is undefined when the directory holds no runtime", () => {
    assert.equal(selectRuntimeVariant(artifactsRoot()), undefined);
  });
});

describe("extractRuntimeWasmFromImage", () => {
  it("extracts the preferred variant into artifacts/from-image/<image>", () => {
    const root = artifactsRoot();
    const result = extractRuntimeWasmFromImage({
      image: "ghcr.io/midnight-ntwrk/midnight-node:latest-amd64",
      artifactsRoot: root,
      docker: fakeDocker({ layout: { amd64: IMAGE_ARTIFACTS_FILE_NAMES } }),
    });

    assert.equal(result.architecture, "amd64");
    assert.equal(
      result.relPath,
      path.join(
        IMAGE_WASM_SUBDIR,
        "ghcr.io_midnight-ntwrk_midnight-node_latest-amd64",
        "midnight_node_runtime.compact.compressed.wasm",
      ),
    );
    assert.ok(fs.existsSync(result.absPath));
    // The sandbox in loadRuntimeWasm resolves relPath against the artifacts root.
    assert.equal(path.resolve(root, result.relPath), result.absPath);
  });

  it("uses the image's own architecture directory", () => {
    const root = artifactsRoot();
    const result = extractRuntimeWasmFromImage({
      image: "node:arm",
      artifactsRoot: root,
      docker: fakeDocker({
        architecture: "arm64",
        layout: { arm64: IMAGE_ARTIFACTS_FILE_NAMES },
      }),
    });
    assert.equal(result.architecture, "arm64");
  });

  it("falls back to the known architectures when inspect gives nothing", () => {
    const root = artifactsRoot();
    const result = extractRuntimeWasmFromImage({
      image: "node:manifest-list",
      artifactsRoot: root,
      docker: fakeDocker({
        inspectFails: true,
        layout: { arm64: IMAGE_ARTIFACTS_FILE_NAMES },
      }),
    });
    assert.equal(result.architecture, "arm64");
  });

  it("removes the created container even when no runtime is found", () => {
    const calls: string[][] = [];
    assert.throws(
      () =>
        extractRuntimeWasmFromImage({
          image: "node:empty",
          artifactsRoot: artifactsRoot(),
          docker: fakeDocker({ layout: { amd64: ["node_image_tag"] }, calls }),
        }),
      /No runtime wasm found in node:empty/,
    );
    assert.ok(calls.some((c) => c[0] === "rm" && c[1] === "-f"));
  });

  it("pulls before creating, so a moved tag isn't served stale", () => {
    const calls: string[][] = [];
    extractRuntimeWasmFromImage({
      image: "node:latest",
      artifactsRoot: artifactsRoot(),
      docker: fakeDocker({ layout: { amd64: IMAGE_ARTIFACTS_FILE_NAMES }, calls }),
    });
    const pullIdx = calls.findIndex((c) => c[0] === "pull");
    const createIdx = calls.findIndex((c) => c[0] === "create");
    assert.ok(pullIdx !== -1 && pullIdx < createIdx);
  });

  it("falls back to the local image when the pull fails", () => {
    const root = artifactsRoot();
    const result = extractRuntimeWasmFromImage({
      image: "local-only:dev",
      artifactsRoot: root,
      docker: fakeDocker({
        pullFails: true,
        layout: { amd64: IMAGE_ARTIFACTS_FILE_NAMES },
      }),
    });
    assert.equal(
      result.relPath,
      path.join(
        IMAGE_WASM_SUBDIR,
        "local-only_dev",
        "midnight_node_runtime.compact.compressed.wasm",
      ),
    );
  });

  it("re-extracts rather than reusing a stale blob from a moved tag", () => {
    const root = artifactsRoot();
    const docker = fakeDocker({ layout: { amd64: IMAGE_ARTIFACTS_FILE_NAMES } });
    const first = extractRuntimeWasmFromImage({
      image: "node:latest",
      artifactsRoot: root,
      docker,
    });
    fs.writeFileSync(first.absPath, "stale");
    const second = extractRuntimeWasmFromImage({
      image: "node:latest",
      artifactsRoot: root,
      docker,
    });
    assert.equal(fs.readFileSync(second.absPath, "utf-8"), "wasm");
  });
});

describe("resolveRuntimeWasmPath", () => {
  it("passes an explicit --wasm path through untouched", () => {
    assert.equal(
      resolveRuntimeWasmPath({
        wasmPath: "upgrade/midnight_node_runtime.compact.wasm",
        env: {},
      }),
      "upgrade/midnight_node_runtime.compact.wasm",
    );
  });

  it("rejects passing both a path and an image", () => {
    assert.throws(
      () =>
        resolveRuntimeWasmPath({
          wasmPath: "a.wasm",
          wasmFromImage: "node:new",
          env: {},
        }),
      /mutually exclusive/,
    );
  });

  it("extracts from the env-derived image when neither flag is given", () => {
    const root = artifactsRoot();
    const rel = resolveRuntimeWasmPath({
      artifactsRoot: root,
      docker: fakeDocker({ layout: { amd64: IMAGE_ARTIFACTS_FILE_NAMES } }),
      env: { NEW_NODE_IMAGE: "node:new" },
    });
    assert.equal(
      rel,
      path.join(
        IMAGE_WASM_SUBDIR,
        "node_new",
        "midnight_node_runtime.compact.compressed.wasm",
      ),
    );
  });

  it("explains what to pass when nothing resolves", () => {
    assert.throws(
      () => resolveRuntimeWasmPath({ env: {} }),
      /No runtime wasm source/,
    );
  });
});
