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

// Per-block digests of the last N canonical blocks (newest first).
// Usage: ts-node digests.ts [ws] [N=20]

import { DEFAULT_WS, describe, run } from "./common";

const count = Number(process.argv[3] ?? 20);

void run(process.argv[2] || DEFAULT_WS, async (api) => {
  let header = await api.rpc.chain.getHeader();
  for (let i = 0; i < count && header.number.toNumber() > 0; i++) {
    console.log(describe(header));
    header = await api.rpc.chain.getHeader(header.parentHash);
  }
});
