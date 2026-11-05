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

import { intEnv } from './cli.mjs';

// Every endpoint comes from the environment. Nothing defaults to a deployed network, so a
// wrong or missing URL fails instead of silently testing something else. Read on use, so a
// missing value fails the command with a FAIL line rather than the import.
const req = (name) => {
  const v = process.env[name];
  if (!v) throw new Error(`missing env ${name}`);
  return v;
};

export const config = {
  get networkId() { return req('MN_NETWORK_ID'); },
  get nodeWs() { return req('MN_NODE_WS'); },
  get indexerHttp() { return req('MN_INDEXER_HTTP'); },
  get indexerWs() { return req('MN_INDEXER_WS'); },
  // One proof server per ledger: ledger 9 rejects DUST-spend proofs from a ledger-8 build.
  get proofServerV8() { return req('MN_PROOF_SERVER_V8'); },
  get proofServerV9() { return req('MN_PROOF_SERVER_V9'); },
  get preForkSpec() { return intEnv('MN_PRE_FORK_SPEC', 1000300, 1); },
  get seed() { return req('MN_SEED'); },
  get outDir() { return req('MN_OUT'); },
  get syncTimeoutMs() { return intEnv('MN_SYNC_TIMEOUT_MS', 180000, 1); },
};

export * from './cli.mjs';
