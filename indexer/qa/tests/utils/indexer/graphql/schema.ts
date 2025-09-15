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

import { z } from 'zod';

export const Hash64 = z
  .string()
  .length(64)
  .regex(/^[a-f0-9]+$/);
export const VarLenghtHex = z.string().regex(/^[a-f0-9]+$/);
export const BlockHeight = z.number().min(0);

export const PartialBlockSchema = z.lazy(() =>
  z.object({
    hash: Hash64,
    height: BlockHeight,
  }),
);

export const BlockSchema = z.lazy(() =>
  z.object({
    hash: Hash64,
    height: BlockHeight,
    timestamp: z.number(),
    parent: PartialBlockSchema,
    transactions: z.array(FullTransactionSchema).min(0),
  }),
);

export const FullTransactionSchema = z.lazy(() =>
  z.object({
    hash: Hash64,
    protocolVersion: z.number(),
    applyStage: z.string(),
    identifiers: z.array(z.string()),
    raw: VarLenghtHex,
    merkleTreeRoot: z.string().regex(/^[a-f0-9]+$/),
    block: PartialBlockSchema,
  }),
);

// Contract related schema validation
const BaseActionSchema = z.object({
  id: z.string(),
  type: z.enum(['CALL', 'DEPLOY', 'UPDATE']),
  timestamp: z.string(),
});

const ContractCallSchema = BaseActionSchema.extend({
  type: z.literal('CALL'),
  method: z.string(),
  args: z.array(z.string()),
});

const ContractDeploySchema = BaseActionSchema.extend({
  type: z.literal('DEPLOY'),
  code: z.string(),
});

const ContractUpdateSchema = BaseActionSchema.extend({
  type: z.literal('UPDATE'),
  patch: z.string(),
});

export const ContractActionSchema = z.discriminatedUnion('type', [
  ContractCallSchema,
  ContractDeploySchema,
  ContractUpdateSchema,
]);
