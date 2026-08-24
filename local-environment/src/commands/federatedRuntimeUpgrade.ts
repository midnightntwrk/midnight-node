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

    // Pre-activation gate: confirm the connected node runs a binary able to decode
    // the new host-response structs (the versioned ledger host function) before the
    // authorize_upgrade motion is submitted. A lagging binary would fail to decode
    // the new runtime's host calls during a rolling upgrade.
    await assertValidatorBinaryCompatible(api, opts);

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
 * Verify the connected node's binary can decode the new host-response structs
 * before the upgrade motion is submitted. The node's active-runtime spec_version
 * is the operational proxy: a node still on a pre-bump runtime has not been rolled
 * to a binary that provides the new versioned ledger host function.
 *
 * Refuses (throws) by default when the node is below the required spec_version;
 * `allowLaggingBinary` downgrades this to a warning for local rehearsals.
 */
async function assertValidatorBinaryCompatible(
  api: ApiPromise,
  opts: FederatedRuntimeUpgradeOptions,
): Promise<void> {
  const nodeVersion = (await api.rpc.system.version()).toString();
  const activeSpecVersion = api.runtimeVersion.specVersion.toNumber();

  console.log(
    `Validator-binary compatibility probe: node version ${nodeVersion}, ` +
      `active runtime spec_version ${activeSpecVersion}`,
  );

  if (opts.requiredNodeSpecVersion === undefined) {
    console.warn(
      "⚠️  No requiredNodeSpecVersion provided; skipping the validator-binary " +
        "spec_version enforcement. Pass it to gate activation on binary capability.",
    );
    return;
  }

  if (activeSpecVersion < opts.requiredNodeSpecVersion) {
    const message =
      `Validator node reports active runtime spec_version ${activeSpecVersion}, ` +
      `below the required ${opts.requiredNodeSpecVersion}: its binary may lack the ` +
      `new ledger host-function version and would fail to decode the new runtime's ` +
      `host calls. Roll the node binaries first, or pass --allow-lagging-binary to override.`;
    if (opts.allowLaggingBinary) {
      console.warn(`⚠️  ${message}`);
    } else {
      throw new Error(`❌ ${message}`);
    }
  }
}

function buildSigners(uris: string[], label: string): KeyringPair[] {
  if (!uris.length) {
    throw new Error(`${label} URIs are required`);
  }

  return uris.map((uri, idx) => createKeyringPair(uri, `${label} ${idx + 1}`));
}

async function proposeCollectiveMotion(
  api: ApiPromise,
  collective: Collective,
  call: SubmittableExtrinsic["method"],
  lengthBound: number,
  proposer: KeyringPair,
  approvalThreshold: number,
): Promise<ProposalInfo> {
  const extrinsic =
    collective === "council"
      ? api.tx.council.propose(approvalThreshold, call, lengthBound)
      : api.tx.technicalCommittee.propose(approvalThreshold, call, lengthBound);

  const result = await signAndWait(
    extrinsic,
    proposer,
    `${collective}.propose`,
  );
  return extractProposalInfo(result, collective);
}

async function voteCollectiveMotion(
  api: ApiPromise,
  collective: Collective,
  proposal: ProposalInfo,
  voters: KeyringPair[],
) {
  const seen = new Set<string>();

  for (const voter of voters) {
    if (seen.has(voter.address)) {
      continue;
    }
    seen.add(voter.address);

    const extrinsic =
      collective === "council"
        ? api.tx.council.vote(
            proposal.proposalHash,
            proposal.proposalIndex,
            true,
          )
        : api.tx.technicalCommittee.vote(
            proposal.proposalHash,
            proposal.proposalIndex,
            true,
          );

    await signAndWait(extrinsic, voter, `${collective}.vote`);
  }
}

async function closeCollectiveProposal(
  api: ApiPromise,
  collective: Collective,
  proposal: ProposalInfo,
  lengthBound: number,
  closer: KeyringPair,
) {
  const weight = api.createType("WeightV2", CLOSE_WEIGHT);

  const extrinsic =
    collective === "council"
      ? api.tx.council.close(
          proposal.proposalHash,
          proposal.proposalIndex,
          weight,
          lengthBound,
        )
      : api.tx.technicalCommittee.close(
          proposal.proposalHash,
          proposal.proposalIndex,
          weight,
          lengthBound,
        );

  await signAndWait(extrinsic, closer, `${collective}.close`);
}

function extractProposalInfo(
  result: ISubmittableResult,
  collective: Collective,
): ProposalInfo {
  const targetSection =
    collective === "council" ? "council" : "technicalcommittee";
  const proposed = result.events.find(
    ({ event }) =>
      event.section.toLowerCase() === targetSection &&
      event.method === "Proposed",
  );

  if (!proposed) {
    throw new Error(`Could not find Proposed event for ${collective}`);
  }

  const proposalIndex = proposed.event.data[1].toPrimitive() as number;
  const proposalHash = proposed.event.data[2].toHex();

  return { proposalHash, proposalIndex };
}

async function getCollectiveMembersCount(
  api: ApiPromise,
  collective: Collective,
): Promise<number> {
  const members =
    collective === "council"
      ? await api.query.council.members()
      : await api.query.technicalCommittee.members();

  return (members.toJSON() as unknown[]).length;
}

function computeTwoThirdsThreshold(
  totalMembers: number,
  label: string,
): number {
  if (totalMembers <= 0) {
    throw new Error(
      `${label} has no on-chain members; cannot compute approval threshold.`,
    );
  }

  return Math.ceil((totalMembers * 2) / 3);
}

function ensureSufficientAuthorities(
  signers: KeyringPair[],
  required: number,
  label: string,
  totalMembers: number,
) {
  const uniqueSigners = new Set(signers.map((signer) => signer.address));
  if (uniqueSigners.size < required) {
    throw new Error(
      `${label} requires at least ${required} unique authorities (2/3 of ${totalMembers}) but only ${uniqueSigners.size} were provided.`,
    );
  }
}
