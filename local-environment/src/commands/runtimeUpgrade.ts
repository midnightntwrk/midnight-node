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

import fs from "fs";
import path from "path";
import BN from "bn.js";
import { ApiPromise, WsProvider } from "@polkadot/api";
import type { SubmittableExtrinsic } from "@polkadot/api/promise/types";
import { Keyring } from "@polkadot/keyring";
import type { KeyringPair } from "@polkadot/keyring/types";
import type { ISubmittableResult } from "@polkadot/types/types";
import { u8aToHex } from "@polkadot/util";
import { blake2AsU8a } from "@polkadot/util-crypto";

import { run } from "./run";
import { RuntimeUpgradeOptions } from "../lib/types";

const DEFAULT_RUNTIME_UPGRADE_DELAY = 15;
const DEFAULT_RPC_URL = "ws://localhost:9944";

interface WasmArtifact {
  path: string;
  hex: string;
  hash: string;
  length: number;
}

export async function runtimeUpgrade(
  namespace: string,
  opts: RuntimeUpgradeOptions,
) {
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
  const provider = new WsProvider(rpcUrl);
  let api: ApiPromise | undefined;

  try {
    api = await ApiPromise.create({ provider });

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
    if (api) {
      await api.disconnect();
    } else {
      provider.disconnect();
    }
  }
}

function loadRuntimeWasm(wasmPath: string): WasmArtifact {
  const resolvedPath = path.resolve(wasmPath);
  if (!fs.existsSync(resolvedPath)) {
    throw new Error(`Unable to find runtime wasm at ${resolvedPath}`);
  }

  const bytes = fs.readFileSync(resolvedPath);
  if (bytes.length === 0) {
    throw new Error(`Runtime wasm at ${resolvedPath} is empty`);
  }

  return {
    path: resolvedPath,
    length: bytes.length,
    hex: u8aToHex(bytes),
    hash: u8aToHex(blake2AsU8a(bytes)),
  };
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

function waitForTargetBlock(api: ApiPromise, target: BN) {
  return new Promise<void>((resolve, reject) => {
    let unsub: (() => void) | undefined;

    const cleanup = () => {
      if (unsub) {
        unsub();
        unsub = undefined;
      }
    };

    api.rpc.chain
      .subscribeNewHeads((header) => {
        const number = header.number.toBn();
        if (number.gte(target)) {
          console.log(
            `Reached block: ${number.toString()} (target: ${target.toString()})`,
          );
          cleanup();
          resolve();
        }
      })
      .then((subscription) => {
        unsub = subscription;
      })
      .catch((error) => {
        cleanup();
        reject(error);
      });
  });
}

async function signAndWait(
  extrinsic: SubmittableExtrinsic,
  signer: KeyringPair,
  label: string,
): Promise<ISubmittableResult> {
  return new Promise((resolve, reject) => {
    let unsub: (() => void) | undefined;

    const cleanup = () => {
      if (unsub) {
        unsub();
        unsub = undefined;
      }
    };

    const fail = (error: unknown) => {
      cleanup();
      reject(error);
    };

    extrinsic
      .signAndSend(signer, { nonce: -1 }, (result: ISubmittableResult) => {
        if (result.dispatchError) {
          let message = result.dispatchError.toString();
          if (result.dispatchError.isModule) {
            const meta = result.dispatchError.registry.findMetaError(
              result.dispatchError.asModule,
            );
            message = `${meta.section}.${meta.name}: ${meta.docs.join(" ")}`;
          }
          fail(new Error(`${label} failed: ${message}`));
          return;
        }

        if (result.status.isInBlock) {
          console.log(
            `${label} included in block ${result.status.asInBlock.toHex()}`,
          );
        }

        if (result.status.isFinalized) {
          console.log(
            `${label} finalized in block ${result.status.asFinalized.toHex()}`,
          );
          cleanup();
          resolve(result);
        }
      })
      .then((subscription) => {
        unsub = subscription;
      })
      .catch(fail);
  });
}

function hasEvent(
  result: ISubmittableResult,
  section: string,
  method: string,
): boolean {
  const targetSection = section.toLowerCase();
  return result.events.some(
    (evt) =>
      evt.event.section.toLowerCase() === targetSection &&
      evt.event.method === method,
  );
}

function createSudoPair(sudoUriOverride?: string): KeyringPair {
  const keyring = new Keyring({ type: "sr25519" });
  const envSudoUri = process.env.SUDO_URI;
  // TODO: remove default seed
  const resolved = (sudoUriOverride ?? envSudoUri ?? "//Alice").trim();

  if (!resolved) {
    throw new Error("Resolved sudo URI is empty");
  }

  console.log(`Using sudo key URI '${resolved}'`);
  return keyring.addFromUri(resolved, { name: "Sudo" });
}

function resolveRpcUrl(candidate?: string): string {
  const trimmed = candidate?.trim();
  if (trimmed) {
    return trimmed;
  }
  return DEFAULT_RPC_URL;
}
