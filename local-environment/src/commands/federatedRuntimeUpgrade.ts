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

import BN from "bn.js";
import type { ApiPromise, WsProvider } from "@polkadot/api";
import type { SubmittableExtrinsic } from "@polkadot/api/promise/types";
import type { KeyringPair } from "@polkadot/keyring/types";
import type { ISubmittableResult } from "@polkadot/types/types";
import { blake2AsHex } from "@polkadot/util-crypto";

import { FederatedRuntimeUpgradeOptions } from "../lib/types";
import {
  createKeyringPair,
  disconnectApi,
  hasEvent,
  signAndWait,
} from "../lib/runtimeUpgradeUtils";
import { prepareRuntimeUpgrade } from "./runtimeUpgradeShared";

type Collective = "council" | "technicalCommittee";

interface ProposalInfo {
  proposalHash: string;
  proposalIndex: number;
}

const CLOSE_WEIGHT = { refTime: new BN(10_000_000_000), proofSize: new BN(65_536) };

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

    const councilMembers = buildSigners(opts.councilUris, "Council");
    const techCommitteeMembers = buildSigners(
      opts.techCommitteeUris,
      "Technical Committee",
    );
    const motionExecutor = createKeyringPair(
      opts.motionExecutorUri,
      "Motion executor",
    );

    const councilMemberCount = await getCollectiveMembersCount(api, "council");
    const councilApprovalThreshold = computeTwoThirdsThreshold(
      councilMemberCount,
      "Council",
    );
    ensureSufficientAuthorities(
      councilMembers,
      councilApprovalThreshold,
      "Council",
      councilMemberCount,
    );

    const techCommitteeMemberCount = await getCollectiveMembersCount(
      api,
      "technicalCommittee",
    );
    const techCommitteeApprovalThreshold = computeTwoThirdsThreshold(
      techCommitteeMemberCount,
      "Technical Committee",
    );
    ensureSufficientAuthorities(
      techCommitteeMembers,
      techCommitteeApprovalThreshold,
      "Technical Committee",
      techCommitteeMemberCount,
    );

    const authorizeUpgradeCall = api.tx.system.authorizeUpgrade(wasm.hash);
    const federatedApproveCall = api.tx.federatedAuthority.motionApprove(
      authorizeUpgradeCall.method,
    );

    const lengthBound = federatedApproveCall.method.encodedLength;
    const motionHash = blake2AsHex(authorizeUpgradeCall.method.toU8a());

    console.log("Submitting Council proposal to approve the motion...");
    const councilProposal = await proposeCollectiveMotion(
      api,
      "council",
      federatedApproveCall.method,
      lengthBound,
      councilMembers[0],
      councilApprovalThreshold,
    );
    await voteCollectiveMotion(
      api,
      "council",
      councilProposal,
      councilMembers,
    );
    await closeCollectiveProposal(
      api,
      "council",
      councilProposal,
      lengthBound,
      councilMembers[0],
    );

    console.log(
      "Submitting Technical Committee proposal to approve the motion...",
    );
    const techProposal = await proposeCollectiveMotion(
      api,
      "technicalCommittee",
      federatedApproveCall.method,
      lengthBound,
      techCommitteeMembers[0],
      techCommitteeApprovalThreshold,
    );
    await voteCollectiveMotion(
      api,
      "technicalCommittee",
      techProposal,
      techCommitteeMembers,
    );
    await closeCollectiveProposal(
      api,
      "technicalCommittee",
      techProposal,
      lengthBound,
      techCommitteeMembers[0],
    );

    console.log("Closing federated motion to execute authorize_upgrade...");
    await signAndWait(
      api.tx.federatedAuthority.motionClose(motionHash),
      motionExecutor,
      "federatedAuthority.motionClose",
    );

    console.log("Applying authorized upgrade...");
    const applyResult = await signAndWait(
      api.tx.system.applyAuthorizedUpgrade(wasm.hex),
      motionExecutor,
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
      : api.tx.technicalCommittee.propose(
          approvalThreshold,
          call,
          lengthBound,
        );

  const result = await signAndWait(extrinsic, proposer, `${collective}.propose`);
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
  const targetSection = collective === "council" ? "council" : "technicalcommittee";
  const proposed = result.events.find(
    ({ event }) =>
      event.section.toLowerCase() === targetSection && event.method === "Proposed",
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

function computeTwoThirdsThreshold(totalMembers: number, label: string): number {
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
