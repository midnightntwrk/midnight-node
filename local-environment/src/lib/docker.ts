// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

import { globSync } from 'glob';
import { existsSync } from 'fs';
import { spawn } from 'child_process';
import path from 'path';

export function runDockerCompose({
    composeFile,
    env,
    profiles = [],
    detach = false
  }: {
    composeFile: string;
    env: Record<string, string>;
    profiles?: string[];
    detach?: boolean;
  }) {
    const dockerArgs = ['compose', '-f', composeFile];
    for (const profile of profiles ?? []) {
      dockerArgs.push('--profile', profile);
    }
    dockerArgs.push('up');
    if (detach) dockerArgs.push('-d');
  
    console.log('Running docker with args:', dockerArgs);
  
    const docker = spawn('docker', dockerArgs, {
      stdio: 'inherit',
      env,
    });
  
    docker.on('exit', (code) => {
      if (code !== 0) {
        console.error(`❌ docker-compose up failed`);
        process.exit(code ?? 1);
      }
    });
  }
