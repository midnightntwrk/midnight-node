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

import log from '@utils/logging/logger';
import '@utils/logging/test-logging-hooks';
import { env } from 'environment/model';

/**
 * Schema-level deprecation guards. The indexer GraphQL schema carries
 * `@deprecated` directives on a handful of fields that we have committed to
 * keeping for one major-version cycle before removal. These tests assert the
 * directives are still attached with the expected reasons, so a regression
 * that strips them (silently breaking the deprecation contract) is caught
 * fast and against any deployed environment.
 */

type DeprecatedFieldExpectation = {
  parentType: string;
  fieldName: string;
  expectedReason: string;
  /** Tracking issue / PR for context in failure messages. */
  source: string;
};

const DEPRECATED_FIELDS: DeprecatedFieldExpectation[] = [
  // Pre-existing v4 deprecations: zswap-prefixed renames across the shielded
  // transaction and progress surfaces.
  {
    parentType: 'RegularTransaction',
    fieldName: 'merkleTreeRoot',
    expectedReason: 'Use zswapMerkleTreeRoot instead',
    source: 'v4 schema',
  },
  {
    parentType: 'RegularTransaction',
    fieldName: 'startIndex',
    expectedReason: 'Use zswapStartIndex instead',
    source: 'v4 schema',
  },
  {
    parentType: 'RegularTransaction',
    fieldName: 'endIndex',
    expectedReason: 'Use zswapEndIndex instead',
    source: 'v4 schema',
  },
  {
    parentType: 'RelevantTransaction',
    fieldName: 'collapsedMerkleTree',
    expectedReason: 'Use zswapCollapsedUpdate instead',
    source: 'v4 schema',
  },
  {
    parentType: 'ShieldedTransactionsProgress',
    fieldName: 'highestEndIndex',
    expectedReason: 'Use highestZswapEndIndex instead',
    source: 'v4 schema',
  },
  {
    parentType: 'ShieldedTransactionsProgress',
    fieldName: 'highestCheckedEndIndex',
    expectedReason: 'Use highestCheckedZswapEndIndex instead',
    source: 'v4 schema',
  },
  {
    parentType: 'ShieldedTransactionsProgress',
    fieldName: 'highestRelevantEndIndex',
    expectedReason: 'Use highestRelevantZswapEndIndex instead',
    source: 'v4 schema',
  },
  // Issue #1032 / PR #1036: deprecate the `fees` wrapper in favour of the new
  // top-level `fee` field on RegularTransaction.
  {
    parentType: 'RegularTransaction',
    fieldName: 'fees',
    expectedReason: 'Use fee instead',
    source: '#1032 / PR #1036',
  },
  // Issue #1032 / PR #1036: deprecate `estimatedFees` since it has been
  // identical to `paidFees` since PR #359.
  {
    parentType: 'TransactionFees',
    fieldName: 'estimatedFees',
    expectedReason: 'Use paidFees instead',
    source: '#1032 / PR #1036',
  },
];

const TYPE_FIELDS_QUERY = `
  query TypeFields($name: String!) {
    __type(name: $name) {
      name
      fields(includeDeprecated: true) {
        name
        isDeprecated
        deprecationReason
      }
    }
  }
`;

type FieldRow = {
  name: string;
  isDeprecated: boolean;
  deprecationReason: string | null;
};

type TypeFieldsResponse = {
  __type: {
    name: string;
    fields: FieldRow[];
  } | null;
};

describe('graphql schema deprecations', () => {
  for (const { parentType, fieldName, expectedReason, source } of DEPRECATED_FIELDS) {
    describe(`${parentType}.${fieldName}`, () => {
      /**
       * @given the deployed indexer GraphQL schema
       * @when we introspect the parent type for the field
       * @then the field is marked deprecated with the agreed reason
       */
      test(`should be marked deprecated with reason "${expectedReason}"`, async () => {
        log.debug(`Introspecting ${parentType} for field ${fieldName} (${source})`);

        const response = await fetch(env.getIndexerHttpBaseURL() + '/api/v4/graphql', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ query: TYPE_FIELDS_QUERY, variables: { name: parentType } }),
        });
        const json = (await response.json()) as { data: TypeFieldsResponse };
        const type = json.data.__type;
        expect(type, `type "${parentType}" not found in schema`).not.toBeNull();
        const field = type!.fields.find((f) => f.name === fieldName);
        expect(field, `field "${parentType}.${fieldName}" not found in schema`).toBeDefined();
        expect(
          field!.isDeprecated,
          `${parentType}.${fieldName} is not marked deprecated (${source})`,
        ).toBe(true);
        expect(
          field!.deprecationReason,
          `${parentType}.${fieldName} deprecation reason changed (${source})`,
        ).toBe(expectedReason);
      });
    });
  }
});
