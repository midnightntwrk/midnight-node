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

import fs from "fs";
import path from "path";
import { ApiPromise, WsProvider } from "@polkadot/api";
import type { SubmittableExtrinsic } from "@polkadot/api/promise/types";
import { Keyring } from "@polkadot/keyring";
import type { KeyringPair } from "@polkadot/keyring/types";
import type { ISubmittableResult } from "@polkadot/types/types";
import { u8aToHex } from "@polkadot/util";
import { blake2AsU8a } from "@polkadot/util-crypto";

export const DEFAULT_RPC_URL = "ws://localhost:9944";

export interface WasmArtifact {
  path: string;
  hex: string;
  hash: string;
  bytes: Uint8Array;
  length: number;
}

export function loadRuntimeWasm(wasmPath: string): WasmArtifact {
  const trimmed = wasmPath?.trim();
  if (!trimmed) throw new Error("Runtime wasm path is required and cannot be empty");
  if (trimmed.includes("\0")) throw new Error("Runtime wasm path cannot include null bytes");

  const allowedRoot = fs.realpathSync(path.resolve(process.cwd(), "artifacts"));
  const candidate = path.resolve(allowedRoot, trimmed);
  const realCandidate = fs.realpathSync(candidate);

  const rel = path.relative(allowedRoot, realCandidate);
  if (rel.startsWith("..") || path.isAbsolute(rel)) {
    throw new Error("Runtime wasm path must be within the artifacts directory");
  }

  if (path.extname(realCandidate) !== ".wasm") {
    throw new Error("Runtime wasm must be a .wasm file");
  }

  const bytes = fs.readFileSync(realCandidate);
  if (bytes.length === 0) throw new Error(`Runtime wasm at ${realCandidate} is empty`);

  const u8 = new Uint8Array(bytes);

  return {
    path: rel,
    length: bytes.length,
    bytes: u8,
    hex: u8aToHex(u8),
    hash: u8aToHex(blake2AsU8a(u8)),
  };
}

export function resolveRpcUrl(candidate?: string): string {
  const trimmed = candidate?.trim();
  if (trimmed) {
    return trimmed;
  }
  return DEFAULT_RPC_URL;
}

export async function createApi(rpcUrl: string): Promise<{
  api: ApiPromise;
  provider: WsProvider;
}> {
  const provider = new WsProvider(rpcUrl);
  const api = await ApiPromise.create({ provider });
  return { api, provider };
}

export async function disconnectApi(
  api?: ApiPromise,
  provider?: WsProvider,
): Promise<void> {
  if (api) {
    await api.disconnect();
  } else if (provider) {
    provider.disconnect();
  }
}

export function createKeyringPair(uri: string, label: string): KeyringPair {
  const trimmed = uri?.trim();
  if (!trimmed) {
    throw new Error(`${label} URI is required and cannot be empty`);
  }

  const keyring = new Keyring({ type: "sr25519" });
  console.log(`Using ${label} key URI '${trimmed}'`);
  return keyring.addFromUri(trimmed, { name: label });
}

export async function signAndWait(
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

export function hasEvent(
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
