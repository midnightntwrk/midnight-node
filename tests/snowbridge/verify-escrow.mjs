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

import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import * as rt from '@midnight-ntwrk/compact-runtime';
import * as escrow from '../../artifacts/snowbridge/compact/escrow/contract/index.js';

const directory = new URL('../../artifacts/snowbridge/compact-ledger/', import.meta.url);
const evidence = JSON.parse(readFileSync(new URL('evidence.json', directory)));
const state = rt.ContractState.deserialize(readFileSync(new URL('escrow-state.bin', directory)));
const record = escrow.ledger(state.data);
assert.equal(record.schemaVersion, 1n);
assert.deepEqual(record.bridgeDomain, new Uint8Array(32).fill(2));
assert.deepEqual(record.tokenDomain, new Uint8Array(32).fill(3));
assert.equal(record.nextSequence, 1n);
assert.equal(record.totalRetired, 40n);
assert.equal(record.records.size(), 1n);
assert.deepEqual(record.records.lookup(0n), { amount: 40n, recipient: new Uint8Array(20).fill(5) });
assert.deepEqual(state.operations(), ['retire']);
assert.deepEqual(Buffer.from(state.operation('retire').verifierKey),
  readFileSync(new URL('../compact/escrow/keys/retire.verifier', directory)));
assert.deepEqual(state.maintenanceAuthority.committee, []);
assert.equal(state.maintenanceAuthority.threshold, 1);
assert.equal(state.maintenanceAuthority.counter, 0n);
assert.deepEqual([...state.balance], [[{ tag: 'unshielded', raw: evidence.manifest.color }, 40n]]);
assert.equal(evidence.stateRoot, evidence.replayedStateRoot);
evidence.committedEscrowDecoded = true;
evidence.redemption = { sequence: 0, amount: 40, recipient: '05'.repeat(20) };
writeFileSync(new URL('evidence.json', directory), JSON.stringify(evidence, null, 2) + '\n');
console.log('Committed escrow verified: record 0 retires 40; balance 40; sealed authority; matching replay root.');
