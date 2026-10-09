// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// M1/M3/M5: runtime, committee storage version and consensus-engine/BABE state.
// Usage: ts-node state.ts [ws]

import { xxhashAsHex } from "@polkadot/util-crypto";
import { DEFAULT_WS, run } from "./common";

void run(process.argv[2] || DEFAULT_WS, async (api) => {
  const version = await api.rpc.state.getRuntimeVersion();
  const scmVersionKey =
    xxhashAsHex("SessionCommitteeManagement", 128) +
    xxhashAsHex(":__STORAGE_VERSION__:", 128).slice(2);
  const best = await api.rpc.chain.getHeader();
  const finalized = await api.rpc.chain.getHeader(
    await api.rpc.chain.getFinalizedHead(),
  );
  const ce = api.query.consensusEngine;
  const babe = api.query.babe;
  console.log({
    node: (await api.rpc.system.version()).toString(),
    spec: version.specVersion.toNumber(),
    scmStorageVersion: (
      (await api.rpc.state.getStorage(scmVersionKey)) as { toHex(): string }
    ).toHex(),
    hasConsensusEngine: !!ce,
    engineState: ce ? (await ce.engineState()).toString() : null,
    babeGenesisSlot: babe ? (await babe.genesisSlot()).toString() : null,
    babeAuthorities: babe
      ? ((await babe.authorities()).toJSON() as [string, number][]).map(
          ([key]) => key,
        )
      : null,
    best: best.number.toNumber(),
    finalized: finalized.number.toNumber(),
  });
});
