// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) Midnight Foundation
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

import { z } from 'zod';

// Dust Output Schema - used by both dust balance and wallet state
export const DustOutputSchema = z.object({
  initial_value: z.number(),
  dust_public: z.string(),
  nonce: z.string(),
  seq: z.number(),
  ctime: z.number(),
  backing_night: z.string(),
  mt_index: z.number(),
});

// Generation Info Schema - used by dust balance
export const GenerationInfoSchema = z.object({
  value: z.number(),
  owner_dust_public_key: z.string(),
  nonce: z.string(),
  dtime: z.number(),
});

// Dust Generation Info Schema - used by dust balance
export const DustGenerationInfoSchema = z.object({
  dust_output: DustOutputSchema,
  generation_info: GenerationInfoSchema,
});

// Dust Balance Schema - validates toolkit dust-balance command output
export const DustBalanceSchema = z.object({
  generation_infos: z.array(DustGenerationInfoSchema),
  // `source` keys are hex-encoded byte strings whose length varies (the value's
  // serialized length is encoded in the leading byte), so keys legitimately come
  // through at different lengths — e.g. 64-char (32-byte) and 66-char (33-byte).
  // Constrain to even-length lowercase hex (whole bytes) only; do NOT pin a length.
  source: z.record(z.string().regex(/^([0-9a-f]{2})+$/), z.number()),
  total: z.number(),
});

// Coin Schema - used by wallet state
export const CoinSchema = z.object({
  nonce: z.string(),
  token_type: z.string(),
  value: z.number(),
  mt_index: z.number(),
});

// UTXO Schema - used by wallet state
export const UtxoSchema = z.object({
  id: z.string(),
  initial_nonce: z.string(),
  value: z.number(),
  user_address: z.string(),
  token_type: z.string(),
  intent_hash: z.string(),
  output_number: z.number(),
});

// Private Wallet State Schema - validates toolkit show-wallet --seed command output
export const PrivateWalletStateSchema = z.object({
  coins: z.record(z.string(), CoinSchema),
  utxos: z.array(UtxoSchema),
  dust_utxos: z.array(DustOutputSchema),
});

// Public Wallet State Schema - validates toolkit show-wallet --address command output
export const PublicWalletStateSchema = z.object({
  coins: z.record(z.string(), CoinSchema),
  utxos: z.array(UtxoSchema),
  dust_utxos: z.array(DustOutputSchema),
});

// Export type aliases inferred from schemas
export type Coin = z.infer<typeof CoinSchema>;
export type Utxo = z.infer<typeof UtxoSchema>;
export type DustOutput = z.infer<typeof DustOutputSchema>;
export type GenerationInfo = z.infer<typeof GenerationInfoSchema>;
export type DustGenerationInfo = z.infer<typeof DustGenerationInfoSchema>;
export type DustBalance = z.infer<typeof DustBalanceSchema>;
export type PrivateWalletState = z.infer<typeof PrivateWalletStateSchema>;
export type PublicWalletState = z.infer<typeof PublicWalletStateSchema>;
