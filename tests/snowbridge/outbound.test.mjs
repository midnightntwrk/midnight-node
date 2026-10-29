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

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { gatewayDispatchLogs } from './outbound-live.mjs';

test('select the result receipt when a live failed release also emits CommandFailed',()=>{
  const logs=JSON.parse(readFileSync(new URL('./fixtures/failed-release-logs.json',import.meta.url)));
  assert.equal(logs.length,2);
  assert.equal(logs[0].address,logs[1].address);
  const selected=gatewayDispatchLogs({logs},logs[0].address.toUpperCase());
  assert.deepEqual(selected,[logs[1]]);
  assert.equal(BigInt('0x'+selected[0].data.slice(66,130)),0n,'actual live failure result');
  assert.deepEqual(gatewayDispatchLogs({logs},'0x'+'00'.repeat(20)),[]);
});
