// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

import BN from "bn.js";
import type { ApiPromise, WsProvider } from "@polkadot/api";
import type { KeyringPair } from "@polkadot/keyring/types";

import { RuntimeUpgradeOptions } from "../lib/types";
import {
  createKeyringPair,
  disconnectApi,
  hasEvent,
  signAndWait,
  waitForTargetBlock,
} from "../lib/runtimeUpgradeUtils";
import { prepareRuntimeUpgrade } from "./runtimeUpgradeShared";

const DEFAULT_RUNTIME_UPGRADE_DELAY = 15;

export async function runtimeUpgrade(
  namespace: string,
  opts: RuntimeUpgradeOptions,
) {
  let api: ApiPromise | undefined;
  let provider: WsProvider | undefined;

  try {
    const prepared = await prepareRuntimeUpgrade(namespace, opts);
    const { wasm } = prepared;
    api = prepared.api;
    provider = prepared.provider;

    const sudoPair = createSudoPair(opts.sudoUri);
    const delayBlocks = resolveDelayBlocks(opts.delayBlocks);

    await waitForDelayBlocks(api, delayBlocks);

    console.log("Submitting sudo runtime upgrade extrinsic...");

    const sudoCall = api.tx.system.setCode(wasm.hex);
    const result = await signAndWait(
      api.tx.sudo.sudo(sudoCall),
      sudoPair,
      "sudo.system.setCode",
    );

    if (!hasEvent(result, "system", "CodeUpdated")) {
      throw new Error(
        "Runtime upgrade executed but System.CodeUpdated event not found.",
      );
    }

    console.log("Runtime upgrade completed successfully.");
  } finally {
    await disconnectApi(api, provider);
  }
}

function resolveDelayBlocks(candidate?: number): number {
  if (candidate === undefined) {
    return DEFAULT_RUNTIME_UPGRADE_DELAY;
  }

  if (!Number.isInteger(candidate)) {
    throw new Error("delayBlocks must be an integer");
  }

  if (candidate < 0) {
    throw new Error("delayBlocks cannot be negative");
  }

  return candidate;
}

async function waitForDelayBlocks(api: ApiPromise, delayBlocks: number) {
  const currentHeader = await api.rpc.chain.getHeader();
  const currentNumber = currentHeader.number.toBn();

  if (delayBlocks === 0) {
    console.log("No block delay requested; submitting upgrade immediately");
    return;
  }

  const targetNumber = currentNumber.add(new BN(delayBlocks));
  console.log(
    `Waiting for block #${targetNumber.toString()} (current #${currentNumber.toString()}, delay ${delayBlocks}) before submitting upgrade`,
  );
  await waitForTargetBlock(api, targetNumber);
}

function createSudoPair(sudoUriOverride?: string): KeyringPair {
  const envSudoUri = process.env.SUDO_URI;
  // TODO: remove default seed
  const resolved = (sudoUriOverride ?? envSudoUri ?? "//Alice").trim();

  if (!resolved) {
    throw new Error("Resolved sudo URI is empty");
  }

  return createKeyringPair(resolved, "Sudo");
}
