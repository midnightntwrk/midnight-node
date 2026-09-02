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

/**
 * Normalize whatever a subscription `error` handler receives into a bare
 * server message string suitable for `expect(...).toBe(...)` /
 * `.toContain(...)` / `.toMatch(...)` assertions.
 *
 * `IndexerWsClient`'s `error` handler can be invoked with three different
 * shapes depending on which graphql-transport-ws route the server takes:
 *
 *   1. `Error` — when the server emits errors inside a `next` frame
 *      (async-graphql 7.2 style); the client wraps `errors[0].message`
 *      in a `new Error(...)` before dispatching.
 *   2. `string` — when the server emits the legacy `type: 'error'` frame
 *      with a plain string payload.
 *   3. `object` — when the legacy `error` frame carries the spec-shaped
 *      `Array<GraphQLError>` payload, or some other structured value.
 *
 * Naive coercion is broken for each of these in a different way:
 *   - `String(new Error('X'))` is `'Error: X'`, not `'X'`.
 *   - `JSON.stringify(new Error('X'))` is `'{}'` (Error fields are not
 *     enumerable).
 * This helper exists so every collector / inline error handler in the
 * subscription tests resolves to the bare message string regardless of
 * which route the server took.
 */
export function extractSubscriptionErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  if (Array.isArray(error)) {
    const first = error[0] as { message?: unknown } | undefined;
    if (first && typeof first.message === 'string') {
      return first.message;
    }
  }
  if (error && typeof error === 'object') {
    const maybe = error as { message?: unknown; errors?: Array<{ message?: unknown }> };
    if (typeof maybe.message === 'string') {
      return maybe.message;
    }
    const first = maybe.errors?.[0];
    if (first && typeof first.message === 'string') {
      return first.message;
    }
  }
  return JSON.stringify(error);
}

/**
 * Build a synthetic `{ data: null, errors: [{ message }] }` payload from
 * whatever `IndexerWsClient`'s `error` handler received. Used in tests that
 * were written before PR #1198 to collect error frames out of the `next`
 * callback: registering this on the `error` callback funnels both routes
 * (legacy `next` errors and the new `errors`-in-`next` route) into the same
 * collected-payload shape, so existing assertions on `payload.errors[0].message`
 * keep working.
 *
 * The generic parameter exists only to let call sites assign the result back
 * into their typed subscription-response array without a separate cast at
 * each call site.
 */
export function buildErrorPayload<T>(error: unknown): T {
  return {
    data: null,
    errors: [{ message: extractSubscriptionErrorMessage(error) }],
  } as unknown as T;
}
