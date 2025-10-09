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

type PromiseExtrinsic = SubmittableExtrinsic;

const DEFAULT_RUNTIME_UPGRADE_DELAY = 30;

export async function runtimeUpgrade(
  namespace: string,
  opts: RuntimeUpgradeOptions,
) {
  const wasmPath = path.resolve(opts.wasmPath);
  if (!fs.existsSync(wasmPath)) {
    throw new Error(`Unable to find runtime wasm at ${wasmPath}`);
  }

  // const wasm = fs.readFileSync(wasmPath);

    // Retrieve the runtime to upgrade
  const wasm = fs.readFileSync(wasmPath).toString('hex');

  if (!wasm.length) {
    throw new Error(`Runtime wasm at ${wasmPath} is empty`);
  }

  const codeHash = blake2AsU8a(wasm);
  console.log(`Loaded runtime wasm (${wasm.length} bytes)`);
  console.log(`Runtime code hash: ${u8aToHex(codeHash)}`);

  console.log("🚀 Ensuring network is running before applying upgrade...");
  await run(namespace, {
    profiles: opts.profiles,
    envFile: opts.envFile,
  });

  const rpcUrl = "ws://localhost:9944";
  console.log(`Connecting to node at ${rpcUrl}`);
  const provider = new WsProvider(rpcUrl);
  let api: ApiPromise | undefined;

  try {
    api = await ApiPromise.create({ provider });

    const keyring = new Keyring({ type: "sr25519" });
    const envSudoUri = process.env.SUDO_URI;
    // TODO: should never default
    const sudoUri = opts.sudoUri ?? envSudoUri ?? "//Alice";
    console.log(`Using sudo key URI '${sudoUri}'`);
    const sudoPair = keyring.addFromUri(sudoUri, { name: "Sudo" });

    const delayBlocks = opts.delayBlocks ?? DEFAULT_RUNTIME_UPGRADE_DELAY;
    if (delayBlocks < 0) {
      throw new Error("delayBlocks cannot be negative");
    }

    const currentHeader = await api.rpc.chain.getHeader();
    const currentNumber = currentHeader.number.toBn();
    const targetNumber = currentNumber.add(new BN(delayBlocks));

    if (delayBlocks > 0) {
      console.log(
        `Waiting for block #${targetNumber.toString()} (current #${currentNumber.toString()}, delay ${delayBlocks}) before submitting upgrade`,
      );
      await waitForTargetBlock(api, targetNumber);
    } else {
      console.log("No block delay requested; submitting upgrade immediately");
    }

    console.log("Submitting sudo runtime upgrade extrinsic...");

    const sudoCall = api.tx.system.setCode(`0x${wasm}`);
    const applyResult = await signAndWait(
      api.tx.sudo.sudo(sudoCall),
      sudoPair,
      "sudo.system.setCode",
    );

    if (!hasEvent(applyResult, "system", "CodeUpdated")) {
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

async function waitForTargetBlock(api: ApiPromise, target: BN) {
  return new Promise<void>((resolve, reject) => {
    let unsub: (() => void) | undefined;

    const handleError = (error: unknown) => {
      unsub?.();
      reject(error);
    };

    (async () => {
      try {
        unsub = await api.rpc.chain.subscribeNewHeads((header) => {
          const number = header.number.toBn();
          if (number.gte(target)) {
            console.log(
              `[runtimeUpgrade] Reached block #${number.toString()} (target #${target.toString()})`,
            );
            unsub?.();
            resolve();
          }
        });
      } catch (error) {
        handleError(error);
      }
    })();
  });
}

async function signAndWait(
  extrinsic: PromiseExtrinsic,
  signer: KeyringPair,
  label: string,
): Promise<ISubmittableResult> {
  return new Promise(async (resolve, reject) => {
    let unsub: (() => void) | undefined;

    const handleError = (err: unknown) => {
      unsub?.();
      reject(err);
    };

    try {
      unsub = await extrinsic.signAndSend(signer, { nonce: -1 }, (result: ISubmittableResult) => {
        if (result.dispatchError) {
          let message = result.dispatchError.toString();
          if (result.dispatchError.isModule) {
            const meta = result.dispatchError.registry.findMetaError(
              result.dispatchError.asModule,
            );
            message = `${meta.section}.${meta.name}: ${meta.docs.join(" ")}`;
          }
          handleError(new Error(`${label} failed: ${message}`));
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
          unsub?.();
          resolve(result);
        }
      });
    } catch (error) {
      handleError(error);
    }
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
