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

// M6: digests, slots and consensusEngine.engineState around the flip.
// Usage: ts-node flip.ts <ws> <from> <to>

import { describe, run } from "./common";

const [url, from, to] = [
  process.argv[2],
  Number(process.argv[3]),
  Number(process.argv[4]),
];

void run(url, async (api) => {
  for (let n = from; n <= to; n++) {
    const hash = await api.rpc.chain.getBlockHash(n);
    const state = await api.query.consensusEngine.engineState.at(hash);
    console.log(
      `[${state.toString()}] ${describe(await api.rpc.chain.getHeader(hash))}`,
    );
  }
  const finalized = await api.rpc.chain.getHeader(
    await api.rpc.chain.getFinalizedHead(),
  );
  console.log({ finalized: finalized.number.toNumber() });
});
