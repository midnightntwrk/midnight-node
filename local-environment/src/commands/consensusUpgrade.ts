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
import type { SubmittableExtrinsic } from "@polkadot/api/promise/types";
import type { Enum } from "@polkadot/types";

import { GovernanceCallOptions } from "../lib/types";
import { disconnectApi } from "../lib/runtimeUpgradeUtils";
import {
  buildFederatedMotionSigners,
  executeFederatedMotion,
} from "../lib/federatedMotion";
import { ensureRunningAndConnect } from "./runtimeUpgradeShared";

/**
 * The `pallet-consensus-engine` governance calls, keyed by their camelCased
 * extrinsic name. Both are gated by `EnsureRoot`, so they cannot be submitted
 * as signed transactions — they must be dispatched as root via a
 * federated-authority motion.
 */
type ConsensusAction = "armBabe" | "scheduleFlip";

/**
 * The `EngineState` variant each action transitions *from*. The pallet treats a
 * call as a no-op from any other state, so we refuse to open a motion unless the
 * engine is here (and gathering collective votes would be wasted).
 */
const REQUIRED_STATE: Record<ConsensusAction, string> = {
  armBabe: "Aura",
  scheduleFlip: "ArmedBabe",
};

const EXPECTED_STATE: Record<ConsensusAction, string> = {
  armBabe: "ArmedBabe",
  scheduleFlip: "ScheduledFlip",
};

interface EngineState {
  /** The active `EngineState` variant name, e.g. "Aura". */
  variant: string;
  /** Full human-readable rendering (variant plus any fields). */
  human: string;
}

async function readEngineState(api: ApiPromise): Promise<EngineState> {
  const state = await api.query.consensusEngine.engineState();
  return {
    variant: (state as unknown as Enum).type,
    human: JSON.stringify(state.toHuman()),
  };
}

async function consensusUpgrade(
  namespace: string,
  opts: GovernanceCallOptions,
  action: ConsensusAction,
) {
  let api: ApiPromise | undefined;
  let provider: WsProvider | undefined;

  try {
    const connection = await ensureRunningAndConnect(namespace, opts);
    api = connection.api;
    provider = connection.provider;

    // Bail out before touching the collectives if the chain state would make the
    // transition a no-op, so we don't gather votes for nothing.
    const required = REQUIRED_STATE[action];
    const before = await readEngineState(api);
    console.log(`Consensus engine state before: ${before.human}`);
    if (before.variant !== required) {
      throw new Error(
        `consensusEngine.${action} requires the engine to be in ${required}, but it is ${before.variant}. ` +
          `Refusing to open a governance motion that would be a no-op.`,
      );
    }

    const signers = buildFederatedMotionSigners(opts);

    const innerCall: SubmittableExtrinsic = api.tx.consensusEngine[action]();
    console.log(
      `Driving federated motion to dispatch consensusEngine.${action} as root...`,
    );

    await executeFederatedMotion(api, innerCall, signers);

    // The consensus-engine calls succeed unconditionally but only transition
    // when the engine is in the expected state, so surface the resulting state
    // rather than assume the flip took effect.
    const after = await readEngineState(api);
    console.log(`Consensus engine state after: ${after.human}`);
    console.log(`consensusEngine.${action} motion completed.`);
    const expected = EXPECTED_STATE[action];
    if (after.variant !== expected) {
      throw new Error(
        `consensusEngine.${action} motion completed but the state is ${after.variant} instead of ${expected}`,
      );
    }
  } finally {
    await disconnectApi(api, provider);
  }
}

export async function consensusUpgradeArmBabe(
  namespace: string,
  opts: GovernanceCallOptions,
) {
  await consensusUpgrade(namespace, opts, "armBabe");
}

export async function consensusUpgradeScheduleFlip(
  namespace: string,
  opts: GovernanceCallOptions,
) {
  await consensusUpgrade(namespace, opts, "scheduleFlip");
}
