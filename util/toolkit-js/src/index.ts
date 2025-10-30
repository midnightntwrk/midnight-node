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

import { Effect, Layer, Logger, LogLevel } from 'effect';
import { Command, CliConfig } from '@effect/cli';
import { NodeContext, NodeRuntime } from "@effect/platform-node";
import { ConfigCompiler, deployCommand, circuitCommand, maintainCommand } from '@midnight-ntwrk/compact-js-command/effect';
import Package from '@midnight-ntwrk/node-toolkit/package.json' with { type: 'json'};

const cli = Command.run(
  Command.make('midnight-node-toolkit-js').pipe(
    Command.withDescription('Provides utilities to execute Compact compiled contracts from the command line.'),
    Command.withSubcommands([
      deployCommand,
      circuitCommand,
      maintainCommand
    ])
  ),
  {
    name: 'Midnight Node Toolkit',
    version: `v${Package.version}`,
    executable: 'midnight-node-toolkit-js'
  }
);

cli(process.argv).pipe(
  Logger.withMinimumLogLevel(LogLevel.None),
  Effect.provide(Layer.mergeAll(
    ConfigCompiler.layer.pipe(Layer.provideMerge(NodeContext.layer)),
    CliConfig.layer({ showBuiltIns: false })
  )),
  NodeRuntime.runMain({ disableErrorReporting: true })
);
