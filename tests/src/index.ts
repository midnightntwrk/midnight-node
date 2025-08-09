// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

import { bech32ToHex, LucidEvoTest } from "./lucid-evo.js";
import {
  cnightPolicyId,
  ownerKeyPaymentBech32,
  redemptionScript,
} from "./static.js";
import {
  generatePrivateKey,
  makeWalletFromPrivateKey,
  validatorToAddress,
} from "@lucid-evolution/lucid";

import { HOST_ADDR } from "./env.js";
import {
  checkCreate,
  checkDeregistration,
  checkMultipleReg,
} from "./substrate.js";

const run = new LucidEvoTest({
  hostAddr: HOST_ADDR,
  privateKeyBech32: ownerKeyPaymentBech32,
  dryRun: false,
});
await run.printUtxos();
await run.printAddresses();

console.log(`cNight Policy ID: ${cnightPolicyId}`);

function newDustHex() {
  return [...Array(32)]
    .map(() => Math.floor(Math.random() * 16).toString(16))
    .join("");
}

const alicePrivateKey = generatePrivateKey();
const aliceWallet = makeWalletFromPrivateKey(
  run.kupmios,
  "Custom",
  alicePrivateKey,
);
const aliceAddress = await aliceWallet.address();
await run.makeCollateral(await aliceWallet.address());

const bobPrivateKey = generatePrivateKey();
const bobWallet = makeWalletFromPrivateKey(
  run.kupmios,
  "Custom",
  bobPrivateKey,
);
const bobAddress = await bobWallet.address();
await run.makeCollateral(await bobWallet.address());

await run.waitForNextCardanoBlock();

await run.send(aliceAddress, { lovelace: 10_000_000n });
await run.send(bobAddress, { lovelace: 10_000_000n });

await run.waitForNextCardanoBlock();

run.selectWallet(alicePrivateKey);
const aliceDustHex = newDustHex();
await run.register(aliceAddress, aliceDustHex);

await run.waitForNextCardanoBlock();

run.selectWallet(bobPrivateKey);
const bobDustHex = newDustHex();
const regUtxos2 = await run.register(bobAddress, bobDustHex);

await run.waitForNextCardanoBlock();

checkMultipleReg([
  {
    cardanoHex: bech32ToHex(aliceAddress),
    dustHex: aliceDustHex,
    label: "Alice",
  },
  {
    cardanoHex: bech32ToHex(bobAddress),
    dustHex: bobDustHex,
    label: "Bob",
  },
]);

await run.waitForNextCardanoBlock();

await run.mintCnight(bobAddress, 100n);
checkCreate(bobDustHex, 100n);

await run.waitForNextCardanoBlock();

await run.mintCnight(aliceAddress, 100n);
checkCreate(aliceDustHex, 100n);

await run.mintCnight(aliceAddress, 101n);
await run.mintCnight(bobAddress, 101n);

checkCreate(aliceDustHex, 101n);
checkCreate(bobDustHex, 101n);

await run.waitForNextCardanoBlock();

await run.deregister(regUtxos2[0]);

checkDeregistration(
  bech32ToHex(bobAddress),
  bobDustHex,
  regUtxos2[0].txHash,
  regUtxos2[0].outputIndex,
);

await run.waitForNextCardanoBlock();

console.log("minting 600 cNight for bob...");

run.selectWallet(ownerKeyPaymentBech32);
const cNightUtxo = await run.mintCnight(bobAddress, 1000n);
const adaUtxo = await run.send(bobAddress, { lovelace: 1_000_000n });
run.selectWallet(bobPrivateKey);

await run.waitForNextCardanoBlock();

await run.printUtxos();
console.log("creating redemption contract...");
const [redeem1] = await run.createRedemptionContract(
  bobAddress,
  200,
  3,
  undefined,
  [cNightUtxo, adaUtxo[0]],
);

await run.waitForNextCardanoBlock();

console.log("executing redemption contract...");
const utxos = await run.executeRedemption(redeem1);
const redeem2 = utxos.find(
  (u) => u.address == validatorToAddress("Custom", redemptionScript),
);
if (!redeem2) {
  throw new Error("no UTxO found for redemption contract");
}

await run.waitForNextCardanoBlock();

console.log("executing redemption contract...");
const utxos2 = await run.executeRedemption(redeem2);

const redeem3 = utxos2.find(
  (u) => u.address == validatorToAddress("Custom", redemptionScript),
);
if (!redeem3) {
  throw new Error("no UTxO found for redemption contract");
}

console.log("executing redemption contract...");
await run.executeRedemption(redeem3);
