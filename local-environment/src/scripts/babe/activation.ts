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

// M3: find the activation block — the first block executed by a runtime >= 3.0.0.
// Usage: ts-node activation.ts [ws]

import { DEFAULT_WS, describe, run } from "./common";

const V3_SPEC = 3_000_000;

void run(process.argv[2] || DEFAULT_WS, async (api) => {
  const specAt = async (n: number) =>
    (
      await api.rpc.state.getRuntimeVersion(await api.rpc.chain.getBlockHash(n))
    ).specVersion.toNumber();
  let lo = 1;
  let hi = (await api.rpc.chain.getHeader()).number.toNumber();
  if ((await specAt(hi)) < V3_SPEC) throw new Error("runtime is not v3 yet");
  // binary search for the first block whose post-state reports spec >= v3
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if ((await specAt(mid)) >= V3_SPEC) hi = mid;
    else lo = mid + 1;
  }
  const setCode = await api.rpc.chain.getHeader(
    await api.rpc.chain.getBlockHash(lo - 1),
  );
  if (!setCode.digest.logs.some((log) => log.isRuntimeEnvironmentUpdated))
    console.warn(
      `#${lo - 1} has no RuntimeEnvironmentUpdated digest — check manually`,
    );
  console.log(`setCode block #${lo - 1}, activation block #${lo}`);
  for (const n of [lo - 1, lo, lo + 1]) {
    console.log(
      describe(
        await api.rpc.chain.getHeader(await api.rpc.chain.getBlockHash(n)),
      ),
    );
  }
});
