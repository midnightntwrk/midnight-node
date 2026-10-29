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
import test from 'node:test';
import { readFileSync } from 'node:fs';
import * as rt from '@midnight-ntwrk/compact-runtime';
import * as escrow from '../../artifacts/snowbridge/compact/escrow/contract/index.js';
import * as consumer from '../../artifacts/snowbridge/compact/consumer/contract/index.js';

const key = '01'.repeat(32);
const address = '04'.repeat(32);
const domain = new Uint8Array(32).fill(2);
const namespace = new Uint8Array(32).fill(3);
const recipient = new Uint8Array(20).fill(5);
const color = rt.rawTokenType(namespace, address);
const token = { tag: 'unshielded', raw: color };
const context = (operation, state) => rt.createCircuitContext(operation, address, key, state, {});
const query = result => result.context.callContext.currentQueryContext;

async function initialEscrow() {
  const contract = new escrow.Contract({});
  const initial = await contract.initialState(rt.createConstructorContext({}, key), domain, namespace);
  return { contract, state: initial.currentContractState };
}

// These test generated circuits and VM effects. Funding and proof verification belong to the ledger gate.
test('compiled escrow schema, empty state, sealed maintenance and sole operation', async () => {
  const expected = readFileSync(new URL('./fixtures/escrow-layout.json', import.meta.url), 'utf8');
  const actualText = readFileSync(new URL('../../artifacts/snowbridge/compact/escrow/compiler/contract-info.json', import.meta.url), 'utf8');
  // Compare text too: JSON numbers cannot exactly represent Uint<128> bounds.
  assert.equal(actualText, expected);
  const actual = JSON.parse(actualText);
  assert.equal(actual['compiler-version'], '0.34.0');
  const { state } = await initialEscrow();
  assert.deepEqual(state.operations(), ['retire']);
  assert.deepEqual(state.maintenanceAuthority.committee, []);
  assert.equal(state.maintenanceAuthority.threshold, 1);
  assert.equal(state.balance.size, 0);
  const ledger = escrow.ledger(state.data);
  assert.equal(ledger.schemaVersion, 1n);
  assert.equal(ledger.nextSequence, 0n);
  assert.equal(ledger.totalRetired, 0n);
  assert.equal(ledger.records.size(), 0n);
  assert.deepEqual(ledger.bridgeDomain, domain);
  assert.deepEqual(ledger.tokenDomain, namespace);
  assert.ok(Object.hasOwn(escrow.expectedVk, 'retire'), 'must compile proving keys, not --skip-zk');
});

test('retirement binds native receipt, recipient and append-only record', async () => {
  const { contract, state } = await initialEscrow();
  const result = await contract.circuits.retire(context('retire', state), 100n, recipient);
  assert.equal(result.result, 0n);
  const q = query(result);
  const ledger = escrow.ledger(q.state);
  assert.equal(ledger.nextSequence, 1n);
  assert.equal(ledger.totalRetired, 100n);
  assert.deepEqual(ledger.records.lookup(0n), {amount: 100n, recipient});
  assert.deepEqual([...q.effects.unshieldedInputs], [[token, 100n]]);
  assert.equal(q.effects.unshieldedMints.size, 0);
  assert.equal(q.effects.unshieldedOutputs.size, 0);
  assert.equal(escrow.ledger(state.data).nextSequence, 0n);
  assert.equal(color, '1faaecfc787ac86a98332ef1e8f5905a540ff62d0ffb881899e0aa001191ce77');
});

test('invalid retirements fail, and the 64-record cap preserves prior records', async () => {
  const { contract, state } = await initialEscrow();
  for (const [amount, target] of [[0n, recipient], [1000000000001n, recipient], [1n, new Uint8Array(20)]]) {
    await assert.rejects(contract.circuits.retire(context('retire', state), amount, target));
  }
  let current = state.data;
  for (let i = 0n; i < 64n; ++i) {
    const result = await contract.circuits.retire(context('retire', current), 1n, recipient);
    assert.equal(result.result, i);
    current = query(result).state;
  }
  await assert.rejects(contract.circuits.retire(context('retire', current), 1n, recipient), /capacity exhausted/);
  const ledger = escrow.ledger(current);
  assert.equal(ledger.records.size(), 64n);
  assert.equal(ledger.totalRetired, 64n);
  assert.deepEqual(ledger.records.lookup(0n), {amount: 1n, recipient});
});

test('independent consumer uses the bridge color for receipt, balance and sending', async () => {
  const contract = new consumer.Contract({});
  const initial = await contract.initialState(rt.createConstructorContext({}, key), Buffer.from(color, 'hex'));
  const state = initial.currentContractState;
  const received = await contract.circuits.deposit(context('deposit', state), 100n);
  assert.deepEqual([...query(received).effects.unshieldedInputs], [[token, 100n]]);
  // This is a VM context fixture; setting a balance here is not ledger funding.
  state.balance = new Map([[token, 100n]]);
  const balance = await contract.circuits.balance(context('balance', state));
  assert.equal(balance.result, 100n);
  const sent = await contract.circuits.withdraw(context('withdraw', state), 40n, new Uint8Array(32).fill(7));
  assert.deepEqual([...query(sent).effects.unshieldedOutputs], [[token, 40n]]);
  assert.equal(query(sent).effects.unshieldedMints.size, 0);
});
