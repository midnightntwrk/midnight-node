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
  getSpecVersion,
  hasEvent,
  signAndWait,
} from "../lib/runtimeUpgradeUtils";
import {
  buildFederatedMotionSigners,
  executeFederatedMotion,
} from "../lib/federatedMotion";
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

    // Capture the runtime spec_version before applying, so we can confirm the
    // upgrade by the version actually changing rather than by decoding the
    // applying block's events. At the upgrade block, events are encoded by the
    // parent (pre-upgrade) runtime, but clients resolve metadata at the block's
    // own (post-upgrade) hash, so `System.CodeUpdated` can fail to decode there
    // and read as absent even though the upgrade executed (see #1960).
    const specVersionBefore = await getSpecVersion(api);

    console.log("Applying authorized upgrade...");
    const applyResult = await signAndWait(
      api.tx.system.applyAuthorizedUpgrade(wasm.hex),
      signers.motionExecutor,
      "system.applyAuthorizedUpgrade",
    );

    const appliedBlockHash = applyResult.status.asInBlock;
    const specVersionAfter = await getSpecVersion(api, appliedBlockHash);

    if (specVersionAfter !== specVersionBefore) {
      console.log(
        `Runtime upgrade completed successfully ` +
          `(spec_version ${specVersionBefore} -> ${specVersionAfter}).`,
      );
    } else if (hasEvent(applyResult, "system", "CodeUpdated")) {
      // spec_version did not change (e.g. --allow-same-version, or a code change
      // that does not bump spec_version). The event shape is unchanged in that
      // case, so decoding the applying block's events is reliable here and
      // System.CodeUpdated is a valid confirmation.
      console.log(
        `Runtime upgrade completed successfully ` +
          `(System.CodeUpdated at spec_version ${specVersionAfter}).`,
      );
    } else {
      throw new Error(
        `Runtime upgrade executed but could not be confirmed: spec_version did ` +
          `not change (still ${specVersionAfter}) and System.CodeUpdated was not ` +
          `found in block ${appliedBlockHash.toHex()}.`,
      );
    }
  } finally {
    await disconnectApi(api, provider);
  }
}
