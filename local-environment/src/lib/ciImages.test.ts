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
import { describe, it } from "node:test";
import { ciImages } from "./ciImages";

function fixture() {
  const services: Record<string, { image?: string; profiles?: string[] }> = {};
  for (const service of [
    "busybox-init",
    "cardano-node-1",
    "cardano-node-socat",
    "kupo",
    "ogmios",
    "db-sync",
    "postgres",
    "postgres-indexer",
    "nats",
  ]) {
    services[service] = { image: `registry.example/${service}:resolved` };
  }
  for (const service of [
    "busybox-init",
    "cardano-node-socat",
    "postgres-indexer",
    "nats",
  ]) {
    services[service].image += `@sha256:${"a".repeat(64)}`;
  }
  services["contract-compiler"] = {};
  services["node"] = { image: "private.example/node:commit" };
  return { services };
}

describe("CI image preloading", () => {
  it("uses compose-resolved overrides and preserves separate digest pins", () => {
    const config = fixture();
    config.services["cardano-node-1"].image = "override.example/cardano:custom";
    const images = ciImages(config, ["private.example/node:commit"]);
    assert.equal(images.CARDANO_IMAGE, "override.example/cardano:custom");
    assert.equal(Object.keys(images).length, 5);
  });

  it("rejects dropping a runtime dependency's digest pin", () => {
    const config = fixture();
    config.services["nats"].image = "nats:latest";
    assert.throws(
      () => ciImages(config, ["private.example/node:commit"]),
      /nats.*retain its digest pin/,
    );
  });

  it("rejects digest preloading unsupported by the pinned EarthBuild release", () => {
    const config = fixture();
    config.services["cardano-node-1"].image += `@sha256:${"a".repeat(64)}`;
    assert.throws(
      () => ciImages(config, ["private.example/node:commit"]),
      /cardano-node-1.*digest-aware/,
    );
  });

  it("ignores unused profiles and build-only services", () => {
    const config = fixture();
    config.services["faucet"] = {
      image: "private.example/faucet:unused",
      profiles: ["withfaucet"],
    };
    assert.doesNotThrow(() =>
      ciImages(config, ["private.example/node:commit"]),
    );
  });

  it("fails when a new active service is not included in the preload list", () => {
    const config = fixture();
    config.services["new-dependency"] = {
      image: "new.example/dependency:latest",
      profiles: ["withindexer"],
    };
    assert.throws(
      () => ciImages(config, ["private.example/node:commit"]),
      /new-dependency.*not preloaded/,
    );
  });

  it("fails when a required dependency has no image", () => {
    const config = fixture();
    delete config.services["db-sync"].image;
    assert.throws(
      () => ciImages(config, ["private.example/node:commit"]),
      /db-sync.*no resolved image/,
    );
  });
});
