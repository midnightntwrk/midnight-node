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

import { DockerComposeEnvironment, type StartedDockerComposeEnvironment, Wait, PullPolicy, ImagePullPolicy } from 'testcontainers';
import {fileURLToPath} from 'url'
import logging from './utils/Logger'
import { StartedGenericContainer } from 'testcontainers/build/generic-container/started-generic-container';

const __filename = fileURLToPath(import.meta.url)
const _logger = logging(__filename)

// Environment variable configuration with fallback to default
const DEFAULT_NODE_PORT_WS = 9944;
const NODE_PORT_WS = parseInt(process.env.NODE_PORT_WS || DEFAULT_NODE_PORT_WS.toString(), 10);

// Utility function to calculate port for multi-node setups
export function getNodePort(nodeIndex: number = 0): number {
  return NODE_PORT_WS + nodeIndex;
}

export async function useTestContainersFixture(dockerComposeLocation: string): Promise<TestContainersFixture> {
  let fixture: TestContainersFixture;

    _logger.info(`Spinning up test environment with base node port: ${NODE_PORT_WS}...`);
    const uid = '1';
    
    const composeEnvironment: StartedDockerComposeEnvironment = await new DockerComposeEnvironment('./', dockerComposeLocation)
      .withWaitStrategy(`node-${uid}`, Wait.forLogMessage("Running JSON-RPC server"))
      .withEnvironment({ 
        TESTCONTAINERS_UID: uid,
        NODE_PORT_WS: NODE_PORT_WS.toString()
      })
      .up();
 
    _logger.info('Test environment started');
    fixture = new TestContainersFixture(composeEnvironment, uid);

  // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
  return fixture;
}

export class TestContainersFixture {
  constructor(
    public readonly composeEnvironment: StartedDockerComposeEnvironment,
    private readonly uid: string,
  ) {}

  public async down(): Promise<void> {
    _logger.info('Tearing down test environment...');
    await this.composeEnvironment.down();
    _logger.info('Test environment torn down');
  }

  // Use environment variable with fallback to default
  public static readonly NODE_PORT_WS = NODE_PORT_WS;
  public static readonly NODE_HOST = "localhost";

  public getNodeWs(): string {
    const node: StartedGenericContainer = this.composeEnvironment.getContainer(`node-${this.uid}`);
    const nodePortWs = node.getMappedPort(TestContainersFixture.NODE_PORT_WS);
    return `ws://${TestContainersFixture.NODE_HOST}:${nodePortWs}`;
  }
}
