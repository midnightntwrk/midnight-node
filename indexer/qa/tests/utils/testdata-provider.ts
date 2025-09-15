// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { env } from '../environment/model';

class TestDataProvider {
  private unshieldedAddresses: Record<string, string>;
  private blocks: Record<string, string>;
  private viewingKeys: Record<string, string[]>;
  private transactions: Record<string, string>;

  constructor() {
    this.unshieldedAddresses = {};
    this.blocks = {};
    this.viewingKeys = {};
    this.transactions = {};
  }

  async init(): Promise<this> {
    const envName = env.getEnvName();
    const blocksDataFile = await import(`../data/static/${envName}/blocks.json`);
    const unshieldedAddressDataFile = await import(
      `../data/static/${envName}/unshielded-addresses.json`
    );
    const viewingKeysDataFile = await import(`../data/static/${envName}/viewing-keys.json`);
    const transactionsDataFile = await import(`../data/static/${envName}/transactions.json`);
    this.unshieldedAddresses = unshieldedAddressDataFile.default;
    this.blocks = blocksDataFile.default;
    this.viewingKeys = viewingKeysDataFile.default;
    this.transactions = transactionsDataFile.default;
    return this;
  }

  getUnshieldedAddress(property: string) {
    if (
      !this.unshieldedAddresses.hasOwnProperty(property) ||
      this.unshieldedAddresses[property] === undefined
    ) {
      throw new Error(
        `Test data provider is missing the unshielded address data for ${property} for ${env.getEnvName()} environment`,
      );
    }
    return this.unshieldedAddresses[property];
  }

  getKnownBlockHash() {
    if (!this.blocks.hasOwnProperty('known-hash') || this.blocks['known-hash'] === undefined) {
      throw new Error(
        `Test data provider is missing the known block hash data for ${env.getEnvName()} environment`,
      );
    }
    return this.blocks['known-hash'];
  }

  getFaucetsViewingKeys() {
    if (
      !this.viewingKeys.hasOwnProperty('pre-fund-faucet') ||
      this.viewingKeys['pre-fund-faucet'] === undefined
    ) {
      throw new Error(
        `Test data provider is missing the pre-fund-faucet viewing keys data for ${env.getEnvName()} environment`,
      );
    }
    return this.viewingKeys['pre-fund-faucet'];
  }

  getKnownTransactionHash() {
    if (
      !this.transactions.hasOwnProperty('known-hash') ||
      this.transactions['known-hash'] === undefined
    ) {
      throw new Error(
        `Test data provider is missing the known transaction hash data for ${env.getEnvName()} environment`,
      );
    }
    return this.transactions['known-hash'];
  }

  getKnownTransactionId() {
    if (
      !this.transactions.hasOwnProperty('known-id') ||
      this.transactions['known-id'] === undefined
    ) {
      throw new Error(
        `Test data provider is missing the known transaction id data for ${env.getEnvName()} environment`,
      );
    }
    return this.transactions['known-id'];
  }

  getFabricatedMalformedHashes() {
    return [
      '0', // half byte
      '000000000000000000000000000000000000000000000000000000000000000G', // Not a valid hex string
      '000000000000000000000000000000000000000000000000000000000000000@', // Not a valid hex string
      '00000000000000000000000000000000000000000000000000000000000062', // 31 bytes (too short)
      '000000000000000000000000000000000000000000000000000000000000000066', // 33 bytes (too long)
    ];
  }

  getFabricatedMalformedIdentifiers() {
    return [
      '000000000000000000000000000000000000000000000000000000000000000G', // Not a valid hex string
      '000000000000000000000000000000000000000000000000000000000000000@', // Not a valid hex string
      '0', // Half byte
    ];
  }

  getFabricatedMalformedHeights() {
    return [
      -1, // negative height
      0.5, // not an integer
      2 ** 32, // 32-bit overflow
    ];
  }
}

const dataProvider = await new TestDataProvider().init();
export default dataProvider;
