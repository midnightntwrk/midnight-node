// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { CompiledContract, ContractExecutable, type Contract } from '@midnight-ntwrk/compact-js/effect';
import { Contract as WelcomeContract_ } from './out/contract/index.js';

// The deployer is the organizer; its key is stored as hex (Uint8Array does not round-trip
// through the JSON private-state file) and converted to bytes in the witness.
type WelcomePrivateState = {
  readonly organizerSecretKey: string | null;
  readonly newOrganizerSecretKey: string | null;
  readonly participantId: string | null;
};

type WelcomeContract = WelcomeContract_<WelcomePrivateState>;
const WelcomeContract = WelcomeContract_;

const witnesses: Contract.Contract.Witnesses<WelcomeContract> = {
  // Returns the caller's secret key when they are an organizer, otherwise `none`.
  local_sk: ({ privateState }) => [
    privateState,
    privateState.organizerSecretKey
      ? { is_some: true, value: new Uint8Array(Buffer.from(privateState.organizerSecretKey, 'hex')) }
      : { is_some: false, value: new Uint8Array(32) },
  ],
  // Supplies the next organizer's secret and switches the local identity to that organizer.
  new_organizer_sk: ({ privateState }) => {
    if (!privateState.newOrganizerSecretKey) {
      throw new Error('No new organizer secret key found');
    }
    const secretKey = privateState.newOrganizerSecretKey;
    return [
      { ...privateState, organizerSecretKey: secretKey, newOrganizerSecretKey: null },
      new Uint8Array(Buffer.from(secretKey, 'hex')),
    ];
  },
  // Records the identity used to check in.
  set_local_id: ({ privateState }, participantId) => [{ ...privateState, participantId }, []],
};

const createInitialPrivateState: () => WelcomePrivateState = () => ({
  organizerSecretKey: '{{ORGANIZER_SK}}',
  newOrganizerSecretKey: '{{NEW_ORGANIZER_SK}}',
  participantId: null,
});

export default {
  contractExecutable: CompiledContract.make<WelcomeContract>(
    'WelcomeContract',
    WelcomeContract,
  ).pipe(
    CompiledContract.withWitnesses(witnesses),
    CompiledContract.withCompiledFileAssets('./out'),
    ContractExecutable.make,
  ),
  createInitialPrivateState,
  config: {
    keys: {
      coinPublic: '{{COIN_PUBLIC}}',
    },
    network: '{{NETWORK}}',
  },
};
