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

import { spawn } from "child_process";

// TODO: Replace with docker library

export interface DockerComposeOptions {
  composeFile: string;
  env: Record<string, string>;
  profiles?: string[];
  detach?: boolean;
}

export function stopDockerCompose(options: DockerComposeOptions) {
  const args = [
    "-f",
    options.composeFile,
    "down",
    "--volumes",
    "--timeout",
    "0",
  ];

  if (options.profiles) {
    for (const profile of options.profiles) {
      args.unshift(`--profile=${profile}`);
    }
  }
  args.unshift("compose");

  const docker = spawn("docker", args, {
    stdio: "inherit",
    env: options.env,
  });

  docker.on("exit", (code) => {
    if (code !== 0) {
      console.error(`❌ docker-compose down failed`);
      process.exit(code ?? 1);
    }
  });
}

export function runDockerCompose(options: DockerComposeOptions) {
  const args = ["-f", options.composeFile, "up"];
  if (options.detach) {
    args.push("--detach");
  }
  if (options.profiles) {
    for (const profile of options.profiles) {
      args.unshift(`--profile=${profile}`);
    }
  }
  args.unshift("compose");

  const docker = spawn("docker", args, {
    stdio: "inherit",
    env: options.env,
  });

  docker.on("exit", (code) => {
    if (code !== 0) {
      console.error(`❌ docker-compose exited with code ${code}`);
      process.exit(code ?? 1);
    }
  });
}
