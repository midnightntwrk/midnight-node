// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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

import {CompiledContract, ContractExecutable, type Contract} from '@midnight-ntwrk/compact-js/effect';
import {Contract as C_} from './out/contract/index.js';

/**
 * A type that describes the private state of the contract.
 */
type PrivateState = {};

// A type alias to the imported Contract type (that binds it to our type of private state).
type MinterContract = C_<PrivateState>;
const MinterContract = C_;

const createInitialPrivateState: () => PrivateState = () => ({});

export default {
    // Use the imports from `@midnight-ntwrk/compact-js/effect` to build an executable contract (an object)
    // that binds the output from `compactc` to the physical and logical assets that are required for its
    // execution.
    contractExecutable: CompiledContract.make<MinterContract>('MinterContract', MinterContract).pipe(
        CompiledContract.withVacantWitnesses,
        CompiledContract.withCompiledFileAssets('./out'),
        ContractExecutable.make
    ),
    createInitialPrivateState,
    // Configuration can also be provided here.
    config: {
        keys: {
            // Seed: 0000000000000000000000000000000000000000000000000000000000000001
            coinPublic: 'aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98',
        },
        network: 'undeployed'
    }
}
