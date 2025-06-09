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

/* eslint-disable @typescript-eslint/no-explicit-any */
import fs from 'fs'
import {fileURLToPath} from 'url'
import logging from './Logger'
const __filename = fileURLToPath(import.meta.url)
const _logger = logging(__filename)

export class Commons {
  public static sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms))
  }

  public static getJsonFromFile(file: string): any {
    return JSON.stringify(JSON.parse(Commons.getTxTemplate(file)))
  }

  public static getTxTemplate(file: string, directory = 'test-contract'): any {
    const filePath = `../../res/${directory}/${file}`
    _logger.info(`Reading file=${filePath}`)
    return fs.readFileSync(filePath).toString('hex')
  }
}
