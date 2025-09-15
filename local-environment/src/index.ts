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


import { Command } from 'commander';
import { run } from './commands/run'
import { stop } from './commands/stop'
import { RunOptions } from './lib/types';

const program = new Command();

program
  .command('run <network>')
  .option('-p, --profiles <profile...>', 'Docker Compose profiles to activate')
  .option('--env-file <path...>', 'specify one or more env files')
  .description('Connect to Kubernetes, extract secrets, then run docker-compose up')
  .action(async (network: string, options: RunOptions) => {
    await run(network, options);
  });
  
  program
  .command('stop <network>')
  .option('-p, --profiles <profile...>', 'Docker Compose profiles to activate')
  .description('Stop the running docker-compose environment for the given network')
  .action(async (network: string, options: RunOptions) => {
    await stop(network, options);
  });

// TODO: program commands for image upgrade, runtime, fork, etc...

program.parse();
