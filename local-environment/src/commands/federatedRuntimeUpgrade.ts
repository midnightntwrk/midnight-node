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

import { FederatedRuntimeUpgradeOptions } from "../lib/types";
import {
  disconnectApi,
  hasEvent,
  signAndWait,
} from "../lib/runtimeUpgradeUtils";
import {
  buildFederatedMotionSigners,
  executeFederatedMotion,
} from "../lib/federatedMotion";
import { isNodeVersionAtLeast } from "../lib/nodeVersion";
import { prepareRuntimeUpgrade } from "./runtimeUpgradeShared";

export async function federatedRuntimeUpgrade(
  namespace: string,
  opts: FederatedRuntimeUpgradeOptions,
) {
  let api: ApiPromise | undefined;
  let provider: WsProvider | undefined;

  try {
    const prepared = await prepareRuntimeUpgrade(namespace, opts);
    api = prepared.api;
    provider = prepared.provider;
    const { wasm } = prepared;

    console.log(`Loaded runtime code hash: ${wasm.hash}`);

    // Pre-activation gate: confirm the connected node runs a binary that provides
    // the ledger host-function versions the new runtime imports before the
    // authorize_upgrade motion is submitted. A lagging binary cannot instantiate
    // the new runtime, so it stops importing blocks once the upgrade applies.
    await assertNodeBinaryCompatible(api, opts);

    const signers = buildFederatedMotionSigners(opts);

    const authorizeUpgradeCall = opts.allowSameVersion
      ? api.tx.system.authorizeUpgradeWithoutChecks(wasm.hash)
      : api.tx.system.authorizeUpgrade(wasm.hash);
    if (opts.allowSameVersion) {
      console.log(
        "Using system.authorizeUpgradeWithoutChecks (--allow-same-version): spec_version check bypassed.",
      );
    }

    await executeFederatedMotion(api, authorizeUpgradeCall, signers);

    console.log("Applying authorized upgrade...");
    const applyResult = await signAndWait(
      api.tx.system.applyAuthorizedUpgrade(wasm.hex),
      signers.motionExecutor,
      "system.applyAuthorizedUpgrade",
    );

    if (!hasEvent(applyResult, "system", "CodeUpdated")) {
      throw new Error(
        "Runtime upgrade executed but System.CodeUpdated event not found.",
      );
    }

    console.log("Runtime upgrade completed successfully.");
  } finally {
    await disconnectApi(api, provider);
  }
}

/**
 * Verify the connected node's binary provides the ledger host-function versions
 * the new runtime imports, before the upgrade motion is submitted. The binary's
 * own version (`system_version`) is the capability signal; the active runtime's
 * spec_version is on-chain state and reads the same on every binary.
 *
 * Only the node the CLI is connected to is probed. Refuses (throws) by default
 * when its binary is older than `requiredNodeVersion`; `allowLaggingBinary`
 * downgrades this to a warning for local rehearsals.
 */
async function assertNodeBinaryCompatible(
  api: ApiPromise,
  opts: FederatedRuntimeUpgradeOptions,
): Promise<void> {
  const nodeVersion = (await api.rpc.system.version()).toString();

  console.log(`Node-binary compatibility probe: node version ${nodeVersion}`);

  if (opts.requiredNodeVersion === undefined) {
    console.warn(
      "⚠️  No requiredNodeVersion provided; skipping the node-binary version " +
        "enforcement. Pass it to gate activation on binary capability.",
    );
    return;
  }

  const atLeast = isNodeVersionAtLeast(nodeVersion, opts.requiredNodeVersion);
  if (atLeast === undefined) {
    throw new Error(
      `❌ Cannot compare node version ${nodeVersion} with required ` +
        `${opts.requiredNodeVersion}: expected major.minor.patch.`,
    );
  }

  if (!atLeast) {
    const message =
      `Connected node binary ${nodeVersion} is older than the required ` +
      `${opts.requiredNodeVersion}: it may lack the ledger host-function versions the ` +
      `new runtime imports and could not instantiate it. Roll the node binaries ` +
      `first, or pass --allow-lagging-binary to override.`;
    if (opts.allowLaggingBinary) {
      console.warn(`⚠️  ${message}`);
    } else {
      throw new Error(`❌ ${message}`);
    }
  }
}
