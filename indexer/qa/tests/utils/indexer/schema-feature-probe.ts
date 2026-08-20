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

import { env } from 'environment/model';
import { retry } from '@utils/retry-helper';

interface IntrospectedField {
  name: string;
  description?: string | null;
  args?: { name: string }[];
}

/**
 * Introspects the deployed schema and returns the fields of a type, or null when
 * the type does not exist. `fieldSelection` adds per-field sub-selections on top
 * of `name` (e.g. `description`, `args { name }`).
 *
 * Throws when the introspection itself does not succeed — a non-2xx response, a
 * GraphQL error, or a response carrying no `data`. Callers read null as "the
 * schema does not have this", so a failed request must never reach them as null:
 * that reports a feature as absent on evidence that says nothing about it.
 *
 * A single blip is retried, since one lost request would otherwise fail every
 * test a probing hook gates.
 *
 * Uses native fetch (pattern of http-compression-probe) because the typed
 * IndexerHttpClient methods are bound to domain queries, not introspection.
 */
async function introspectTypeFields(
  typeName: string,
  fieldSelection: string,
): Promise<IntrospectedField[] | null> {
  return retry(() => introspectTypeFieldsOnce(typeName, fieldSelection), {
    maxRetries: 1,
    delayMs: 1000,
    retryLabel: `introspection of type ${typeName}`,
  });
}

/** One introspection attempt. See `introspectTypeFields` for the contract. */
async function introspectTypeFieldsOnce(
  typeName: string,
  fieldSelection: string,
): Promise<IntrospectedField[] | null> {
  const apiVersion = process.env.INDEXER_API_VERSION?.trim() || 'v4';
  const url = `${env.getIndexerHttpBaseURL()}/api/${apiVersion}/graphql`;

  const response = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      query: `{ __type(name: "${typeName}") { fields { name ${fieldSelection} } } }`,
    }),
    signal: AbortSignal.timeout(30_000),
  });

  if (!response.ok) {
    throw new Error(
      `Introspection of type ${typeName} against ${url} returned HTTP ` +
        `${response.status} ${response.statusText}`,
    );
  }

  const body = (await response.json()) as {
    data?: { __type?: { fields?: IntrospectedField[] } };
    errors?: { message?: string }[];
  };

  if (body.errors?.length) {
    const messages = body.errors.map((error) => error.message ?? '<no message>').join('; ');
    throw new Error(`Introspection of type ${typeName} against ${url} failed: ${messages}`);
  }

  if (body.data === undefined) {
    throw new Error(`Introspection of type ${typeName} against ${url} returned no data`);
  }

  // Only now does null mean what the callers take it to mean: the schema served by
  // this environment has no such type.
  return body.data.__type?.fields ?? null;
}

/**
 * Introspects the deployed schema and returns the description of a `Block` field,
 * or null when the field does not exist.
 */
export async function fetchBlockFieldDescription(fieldName: string): Promise<string | null> {
  const fields = await introspectTypeFields('Block', 'description');
  return fields?.find((f) => f.name === fieldName)?.description ?? null;
}

/**
 * Whether the deployed indexer serves per-block dust Merkle tree roots.
 *
 * Up to and including 4.3.3 the `Block.dust*MerkleTreeRoot` fields are documented
 * (and resolved) "at the latest indexed state" — the tip's roots for every block.
 * Since the per-block change the deployed schema documents them "at this block".
 * The description is the only observable version marker: field name and type are
 * identical on both sides.
 */
export async function isPerBlockDustRootsSupported(): Promise<boolean> {
  const description = await fetchBlockFieldDescription('dustGenerationMerkleTreeRoot');
  return description !== null && description.includes('at this block');
}

/**
 * Whether the deployed indexer serves the block-hash `dustGenerations` signature.
 *
 * Up to and including 4.3.3 the subscription takes `(dustAddress, startIndex,
 * endIndex)`; the block-hash sync replaced those with `(dustAddress, blockHash,
 * dtimeCutoffHeight)`. The argument names are the only observable marker — the
 * field name and its event union are identical on both sides. Without this probe
 * the block-hash subscription document fails GraphQL validation on a pre-4.4
 * environment, turning a missing surface into a suite of hard failures.
 *
 * A probe that cannot run at all throws with its own name in the message: the
 * suite gates every test on this one call, so a bare `TypeError: fetch failed`
 * across twelve tests reads like a dustGenerations regression rather than an
 * unreachable indexer.
 */
export async function isBlockHashDustGenerationsSupported(): Promise<boolean> {
  const fields = await introspectTypeFields('Subscription', 'args { name }').catch((error) => {
    throw new Error(
      `dustGenerations surface probe failed against ${env.getIndexerHttpBaseURL()}: ` +
        `${(error as Error).message}`,
      { cause: error },
    );
  });
  const dustGenerations = fields?.find((f) => f.name === 'dustGenerations');
  return dustGenerations?.args?.some((arg) => arg.name === 'blockHash') ?? false;
}
