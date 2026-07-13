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

import type { ApiPromise, WsProvider } from "@polkadot/api";

import { run } from "./run";
import { RuntimeUpgradeBaseOptions } from "../lib/types";
import {
  createApi,
  loadRuntimeWasm,
  resolveRpcUrl,
} from "../lib/runtimeUpgradeUtils";

export interface PreparedRuntimeUpgrade {
  wasm: ReturnType<typeof loadRuntimeWasm>;
  api: ApiPromise;
  provider: WsProvider;
  rpcUrl: string;
}

export async function prepareRuntimeUpgrade(
  namespace: string,
  opts: RuntimeUpgradeBaseOptions,
): Promise<PreparedRuntimeUpgrade> {
  const wasm = loadRuntimeWasm(opts.wasmPath);

  console.log(`Loaded runtime wasm from ${wasm.path} (${wasm.length} bytes)`);
  console.log(`Runtime code hash: ${wasm.hash}`);

  if (opts.skipRun) {
    console.log("Skipping docker-compose bring-up (--skip-run)");
  } else {
    console.log("Ensuring network is running before applying upgrade...");
    await run(namespace, {
      profiles: opts.profiles,
      envFile: opts.envFile,
      fromSnapshot: opts.fromSnapshot,
    });
  }

  const rpcUrl = resolveRpcUrl(opts.rpcUrl);
  console.log(`Connecting to node at ${rpcUrl}`);
  const { api, provider } = await createApi(rpcUrl);

  return { wasm, api, provider, rpcUrl };
}
