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
import net from "net";
import path from "path";
import { promises as dnsPromises } from "dns";
import { ApiPromise, WsProvider } from "@polkadot/api";
import type { SubmittableExtrinsic } from "@polkadot/api/promise/types";
import { Keyring } from "@polkadot/keyring";
import type { KeyringPair } from "@polkadot/keyring/types";
import type { ISubmittableResult } from "@polkadot/types/types";
import { u8aToHex } from "@polkadot/util";
import { blake2AsU8a } from "@polkadot/util-crypto";

// Prefer an explicit IPv4 host over `localhost`: under the Node 17+ resolver
// order `localhost` resolves to ::1 first, and if the IPv6 loopback path to the
// node's published port is not routable the WsProvider (which has no connect
// timeout of its own) retries forever instead of falling back to IPv4.
export const DEFAULT_RPC_URL = "ws://127.0.0.1:9944";

// Fail a stuck connection fast with an actionable error rather than letting the
// WsProvider auto-reconnect indefinitely (which otherwise hangs the whole run).
export const API_CONNECT_TIMEOUT_MS = 60_000;

export interface WasmArtifact {
  path: string;
  hex: string;
  hash: string;
  bytes: Uint8Array;
  length: number;
}

export function loadRuntimeWasm(wasmPath: string): WasmArtifact {
  const trimmed = wasmPath?.trim();
  if (!trimmed)
    throw new Error("Runtime wasm path is required and cannot be empty");
  if (trimmed.includes("\0"))
    throw new Error("Runtime wasm path cannot include null bytes");

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
  if (bytes.length === 0)
    throw new Error(`Runtime wasm at ${realCandidate} is empty`);

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

// Print the definite reason a connect failed: what the host resolved to (and in
// what order) plus a raw per-address TCP probe. Distinguishes "IPv6 path dead,
// IPv4 works" from "nothing is listening" without waiting out a job timeout.
async function logConnectDiagnostics(rpcUrl: string): Promise<void> {
  const url = new URL(rpcUrl);
  const port = Number(url.port);
  const addrs = await dnsPromises
    .lookup(url.hostname, { all: true, verbatim: true })
    .catch(() => [] as { address: string; family: number }[]);
  if (addrs.length) {
    console.error(
      `DNS ${url.hostname} (verbatim order): ${addrs
        .map((a) => `${a.address}(v${a.family})`)
        .join(", ")}`,
    );
  }
  const targets = addrs.length ? addrs.map((a) => a.address) : [url.hostname];
  for (const address of targets) {
    const result = await new Promise<string>((resolve) => {
      const socket = net.connect({ host: address, port });
      const done = (msg: string) => {
        socket.destroy();
        resolve(msg);
      };
      socket.setTimeout(4000);
      socket.once("connect", () => done("OK"));
      socket.once("timeout", () => done("TIMEOUT"));
      socket.once("error", (e: NodeJS.ErrnoException) =>
        done(`FAIL (${e.code ?? e.message})`),
      );
    });
    console.error(`TCP ${address}:${port} -> ${result}`);
  }
}

export async function createApi(
  rpcUrl: string,
  connectTimeoutMs: number = API_CONNECT_TIMEOUT_MS,
): Promise<{
  api: ApiPromise;
  provider: WsProvider;
}> {
  const provider = new WsProvider(rpcUrl);
  // Surface socket-level connect activity so a stuck connection is never silent.
  provider.on("error", (e) =>
    console.error(
      `WsProvider error (${rpcUrl}): ${(e as Error)?.message ?? e}`,
    ),
  );
  provider.on("disconnected", () =>
    console.warn(`WsProvider disconnected (${rpcUrl})`),
  );
  provider.on("connected", () =>
    console.log(`WsProvider connected (${rpcUrl})`),
  );

  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => {
      reject(
        new Error(
          `Timed out after ${connectTimeoutMs}ms connecting to ${rpcUrl}. ` +
            `Is the node RPC reachable there? If the URL uses 'localhost' it may ` +
            `resolve to IPv6 (::1); try an explicit IPv4 host, e.g. ws://127.0.0.1:<port>.`,
        ),
      );
    }, connectTimeoutMs);
  });

  try {
    const api = await Promise.race([ApiPromise.create({ provider }), timeout]);
    return { api, provider };
  } catch (err) {
    await logConnectDiagnostics(rpcUrl).catch(() => {
      // diagnostics are best-effort
    });
    // Tear down the socket so a failed connect doesn't keep reconnecting in the
    // background and hold the process open.
    try {
      await provider.disconnect();
    } catch {
      // best-effort cleanup
    }
    throw err;
  } finally {
    if (timer) {
      clearTimeout(timer);
    }
  }
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
