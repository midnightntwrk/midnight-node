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

const IGNORED_PATTERNS = [
  '@oneOf',
  '@skip',
  '@include',
  '@deprecated',
  '@specifiedBy',
  'description',
];

export function cleanSchemaSDL(sdl: string): string[] {
  return sdl
    .split('\n')
    .map((line) =>
      IGNORED_PATTERNS.reduce((acc, pattern) => acc.replace(pattern, ''), line.trim())
        // Collapse multiple spaces
        .replace(/\s+/g, ' ')
        // Remove space before opening brace
        .replace(/ \{/g, '{'),
    )
    .filter((line) => line !== '');
}
