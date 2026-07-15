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
import { RunOptions, RuntimeUpgradeBaseOptions } from "../lib/types";
import {
  createApi,
  loadRuntimeWasm,
  resolveRpcUrl,
} from "../lib/runtimeUpgradeUtils";

export interface NetworkConnection {
  api: ApiPromise;
  provider: WsProvider;
  rpcUrl: string;
}

export interface PreparedRuntimeUpgrade extends NetworkConnection {
  wasm: ReturnType<typeof loadRuntimeWasm>;
}

/**
 * Optionally bring the network up via docker-compose, then open an API
 * connection to the target node. Shared by every governance command that needs
 * a running node to submit against.
 */
export async function ensureRunningAndConnect(
  namespace: string,
  opts: RunOptions & { skipRun?: boolean; rpcUrl?: string },
): Promise<NetworkConnection> {
  if (opts.skipRun) {
    console.log("Skipping docker-compose bring-up (--skip-run)");
  } else {
    console.log("Ensuring network is running before submitting...");
    await run(namespace, {
      profiles: opts.profiles,
      envFile: opts.envFile,
      fromSnapshot: opts.fromSnapshot,
    });
  }

  const rpcUrl = resolveRpcUrl(opts.rpcUrl);
  console.log(`Connecting to node at ${rpcUrl}`);
  const { api, provider } = await createApi(rpcUrl);

  return { api, provider, rpcUrl };
}

export async function prepareRuntimeUpgrade(
  namespace: string,
  opts: RuntimeUpgradeBaseOptions,
): Promise<PreparedRuntimeUpgrade> {
  const wasm = loadRuntimeWasm(opts.wasmPath);

  console.log(`Loaded runtime wasm from ${wasm.path} (${wasm.length} bytes)`);
  console.log(`Runtime code hash: ${wasm.hash}`);

  const connection = await ensureRunningAndConnect(namespace, opts);

  return { wasm, ...connection };
}
