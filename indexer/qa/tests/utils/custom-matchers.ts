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

import { expect } from 'vitest';
import { GraphQLResponse } from './indexer/indexer-types';

declare module 'vitest' {
  interface Assertion<T> {
    toBeSuccess(): void;
    toBeError(): void;
  }
}

expect.extend({
  toBeSuccess(received: GraphQLResponse<unknown>) {
    const pass = received.errors === undefined && received.data != null;

    if (pass) {
      return {
        message: () => `expected response not to contain valid GraphQL data`,
        pass: true,
      };
    } else {
      return {
        message: () =>
          `expected response to contain valid GraphQL data, but got ${JSON.stringify(received, null, 2)}`,
        pass: false,
      };
    }
  },

  toBeError(received: GraphQLResponse<any>) {
    const pass =
      received.errors != null &&
      received.errors.length === 1 &&
      received.errors[0].message != null &&
      received.data == null;

    if (pass) {
      return {
        message: () => `expected response not to contain a GraphQL error`,
        pass: true,
      };
    } else {
      return {
        message: () =>
          `expected response to contain a GraphQL error, but got ${JSON.stringify(received, null, 2)}`,
        pass: false,
      };
    }
  },
});
