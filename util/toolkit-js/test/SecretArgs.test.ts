// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { describe, expect, it } from 'vitest';

import { expandSecretFileArgs } from '../src/secret-args.js';

const SECRETS: Record<string, string> = {
  '/tmp/signing': 'deadbeef01\n',
  '/tmp/authority': 'cafebabe02\n',
};
const readSecret = (path: string): string => {
  const secret = SECRETS[path];
  if (secret === undefined) throw new Error(`unexpected path ${path}`);
  return secret;
};

describe('expandSecretFileArgs', () => {
  it('expands --signing-file into --signing with trimmed file contents', () => {
    const argv = ['node', 'bin.js', 'maintain', 'circuit', '--signing-file', '/tmp/signing', 'addr', 'my_circuit'];
    expandSecretFileArgs(argv, readSecret);
    expect(argv).toEqual(['node', 'bin.js', 'maintain', 'circuit', '--signing', 'deadbeef01', 'addr', 'my_circuit']);
  });

  it('expands --new-authority-file positionally, in place', () => {
    const argv = ['node', 'bin.js', 'maintain', 'contract', '--signing-file', '/tmp/signing', 'addr', '--new-authority-file', '/tmp/authority'];
    expandSecretFileArgs(argv, readSecret);
    expect(argv).toEqual(['node', 'bin.js', 'maintain', 'contract', '--signing', 'deadbeef01', 'addr', 'cafebabe02']);
  });

  it('leaves argv without secret-file flags untouched', () => {
    const argv = ['node', 'bin.js', 'deploy', '-c', 'config.ts', '--coin-public', 'aabb'];
    expandSecretFileArgs(argv, readSecret);
    expect(argv).toEqual(['node', 'bin.js', 'deploy', '-c', 'config.ts', '--coin-public', 'aabb']);
  });

  it('ignores a trailing flag with no value (CLI will reject it)', () => {
    const argv = ['node', 'bin.js', 'maintain', 'circuit', '--signing-file'];
    expandSecretFileArgs(argv, readSecret);
    expect(argv).toEqual(['node', 'bin.js', 'maintain', 'circuit', '--signing-file']);
  });
});
