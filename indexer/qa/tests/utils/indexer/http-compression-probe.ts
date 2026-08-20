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

export interface CompressionProbeResult {
  status: number;
  // The raw Content-Encoding header value, or null if the server sent none.
  contentEncoding: string | null;
  data: unknown;
}

// Large enough to reliably exceed tower-http's default minimum-size threshold
// for compression (SizeAbove(32), i.e. 32 bytes). A minimal domain query
// (e.g. `{ block { hash } }`) can return a body smaller than the threshold,
// causing the server to skip compression even when the client advertises
// Accept-Encoding — which would make the gzip/br/zstd assertions below fail
// spuriously.
const INTROSPECTION_QUERY = `{
  __schema {
    types {
      name
      fields {
        name
        type { name kind ofType { name kind } }
      }
    }
  }
}`;

/**
 * Sends a raw GraphQL POST and returns the HTTP status, Content-Encoding
 * header, and parsed body.
 *
 * Uses native fetch rather than graphql-request so that response headers
 * remain accessible — graphql-request parses and discards them before
 * surfacing the response to callers.
 *
 * Note on fetch decompression: gzip, deflate and brotli are decompressed
 * transparently by both Node (undici) and Bun. `zstd`, however, is NOT
 * auto-decompressed by Node's fetch on the versions we run on — undici only
 * gained zstd support in v7.11 (Node >= 24.4.0), and the entire Node 22.x /
 * 23.x line ships undici 6.x/early-7.x without it (Bun does decompress zstd).
 * So for zstd we read the raw bytes and decompress them ourselves (see below).
 * The Content-Encoding header is preserved in `response.headers` regardless and
 * reflects what the server actually sent.
 *
 * Note on the zstd runtime requirement: `zstdDecompressSync` was only added to
 * `node:zlib` in Node 22.15.0. We import it lazily, inside the zstd branch, so
 * the gzip / brotli / identity probes keep working on earlier Node 22.x
 * releases — only the zstd decompression path needs Node >= 22.15.0 (see the
 * prerequisites note in qa/tests/README.md).
 *
 * Note on identity responses: undici unconditionally appends its own
 * `Accept-Encoding` (gzip, deflate, br) to every outgoing request. To test
 * the server's identity (uncompressed) path, pass `Accept-Encoding: identity`
 * explicitly — that overrides undici's default and instructs the server not
 * to compress.
 */
export async function probeGraphQLCompression(
  acceptEncoding?: string,
  query: string = INTROSPECTION_QUERY,
): Promise<CompressionProbeResult> {
  const apiVersion = process.env.INDEXER_API_VERSION?.trim() || 'v4';
  const url = `${env.getIndexerHttpBaseURL()}/api/${apiVersion}/graphql`;

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  if (acceptEncoding !== undefined) {
    headers['Accept-Encoding'] = acceptEncoding;
  }

  const response = await fetch(url, {
    method: 'POST',
    headers,
    body: JSON.stringify({ query }),
    signal: AbortSignal.timeout(30_000),
  });

  const contentEncoding = response.headers.get('content-encoding');

  // Node's fetch (undici < 7.11, i.e. all Node 22.x/23.x) does NOT
  // auto-decompress `Content-Encoding: zstd`, so `response.json()` would try to
  // parse raw zstd bytes and throw. Read the body as bytes and decompress zstd
  // ourselves, but only when the server actually sent zstd AND the payload
  // still carries the zstd magic number (0x28 B5 2F FD). The magic-number guard
  // makes this a no-op on runtimes that already decompressed (Bun, Node >=
  // 24.4.0), so the gzip / brotli / identity paths are unaffected.
  const buffer = Buffer.from(await response.arrayBuffer());
  const isRawZstd =
    contentEncoding === 'zstd' &&
    buffer.length >= 4 &&
    buffer[0] === 0x28 &&
    buffer[1] === 0xb5 &&
    buffer[2] === 0x2f &&
    buffer[3] === 0xfd;

  let payload = buffer;
  if (isRawZstd) {
    // Import lazily so the module still loads on Node 22.x < 22.15, where
    // `zstdDecompressSync` does not exist — only this branch needs it.
    const { zstdDecompressSync } = await import('node:zlib');
    if (typeof zstdDecompressSync !== 'function') {
      throw new Error(
        'Received a zstd-compressed response but node:zlib.zstdDecompressSync ' +
          'is unavailable; Node >= 22.15.0 is required to decompress zstd.',
      );
    }
    payload = zstdDecompressSync(buffer);
  }
  const data = JSON.parse(payload.toString('utf8'));

  return {
    status: response.status,
    contentEncoding,
    data,
  };
}
