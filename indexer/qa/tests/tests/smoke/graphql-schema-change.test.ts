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

import fs from 'fs';
import { diffLines } from 'diff';
import { env } from 'environment/model';
import { cleanSchemaSDL } from '@utils/graphql-utils';
import { buildClientSchema, lexicographicSortSchema, buildSchema, printSchema } from 'graphql';

describe('GraphQL Schema Stability Check', () => {
  it.skip('should not change the schema unexpectedly', async () => {
    //compareSchemas('reference/introspection-query.graphql', `${env.getIndexerHttpBaseURL()}/api/v1/graphql`);

    // Load a static version of the introspection query from file
    const introspectionQeury = fs.readFileSync('reference/introspection-query.graphql', 'utf-8');

    // Fetch a schema from the GraphQL server via introspection
    const response = await fetch(`${env.getIndexerHttpBaseURL()}/api/v1/graphql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        query: introspectionQeury,
      }),
    });

    const actualSchema = lexicographicSortSchema(buildClientSchema((await response.json()).data));

    // Import the latest known schema from file
    const schemaFromFile = fs.readFileSync('reference/schema-v1.graphql', 'utf-8');
    const expectedSchema = lexicographicSortSchema(buildSchema(schemaFromFile));

    // Use cleanSchemaSDL to cleanup all the noise and focus on the actual content
    const cleanExpectedSchema = cleanSchemaSDL(printSchema(expectedSchema));
    const cleanActualSchema = cleanSchemaSDL(printSchema(actualSchema));

    // Calculate the diff after the clanup
    const diff = diffLines(cleanExpectedSchema.join('\n'), cleanActualSchema.join('\n'));

    const meaningfulDiffs = diff.filter((line) => line.added || line.removed);

    // Print and fail if there are breaking changes in the schema
    if (meaningfulDiffs.length > 0) {
      console.log('Schema changed:');
      for (const line of meaningfulDiffs) {
        console.log((line.added ? '+ ' : '- ') + line.value);
      }
      throw new Error('Unexpected schema change detected.');
    }
  });
});
