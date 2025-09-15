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

import { bech32m } from 'bech32';

// Encode a Bech32m address with a new prefix
export function encodeBech32mWithPrefix(address: string, prefix: string) {
  const decoded = bech32m.decode(address);
  return bech32m.encode(prefix, decoded.words);
}

// Decode a Bech32m address
export function decodeBech32m(address: string) {
  return bech32m.decode(address);
}

/**
 * Generates a synthetic viewing key for testing purposes
 * @param networkPrefix - The network prefix (e.g., 'dev1', 'test1')
 * @returns A synthetic viewing key that will be rejected as invalid
 */
export function generateSyntheticViewingKey(networkPrefix: string = 'dev1'): string {
  // Create a synthetic payload with random data
  // This will be rejected by the indexer but maintains proper bech32 format
  const syntheticWords = Array.from({ length: 50 }, () => Math.floor(Math.random() * 32));

  // Use the same prefix as real viewing keys but with synthetic data
  const prefix = `mn_shield-esk_${networkPrefix}`;

  return bech32m.encode(prefix, syntheticWords);
}

/**
 * Encodes a viewing key by scrambling the payload while maintaining network ID
 * @param viewingKey - The original viewing key to scramble
 * @returns A new viewing key with scrambled payload but same network ID
 */
export function encodeScrambledAddress(viewingKey: string): string {
  // Decode the viewing key to get prefix and data
  const decoded = bech32m.decode(viewingKey);
  if (!decoded) {
    throw new Error('Invalid bech32 viewing key');
  }

  const { prefix, words } = decoded;

  // For viewing keys, we need to preserve the network ID (first few words)
  // and only scramble the payload portion
  const networkIdWords = words.slice(0, 2); // First 2 words typically contain network ID
  const payloadWords = words.slice(2); // Rest is the payload

  // Scramble only the payload words
  const scrambledPayload = [...payloadWords];
  for (let i = scrambledPayload.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [scrambledPayload[i], scrambledPayload[j]] = [scrambledPayload[j], scrambledPayload[i]];
  }

  // Combine network ID with scrambled payload
  const scrambledWords = [...networkIdWords, ...scrambledPayload];

  // Encode the scrambled words back to bech32
  return bech32m.encode(prefix, scrambledWords);
}
