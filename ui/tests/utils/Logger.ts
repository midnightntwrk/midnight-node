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

import path from 'path'
import pkg from 'winston'
const {createLogger, format, transports} = pkg
import DailyRotateFile from 'winston-daily-rotate-file'

const {combine, timestamp, printf} = format

const baseFormat = combine(
  timestamp({format: 'YYYY-MM-DD HH:mm:ss,SSS'}),
  printf(({level, message, timestamp, ...metadata}) => {
    return `${timestamp} ${level.toUpperCase()} [${metadata.class}] - ${message}`
  }),
)

const options = {
  level: 'debug',
  transports: [
    new transports.Console({
      level: 'info',
    }),
    new DailyRotateFile({
      level: 'debug',
      filename: 'logs/midnight-e2e-automation.%DATE%.log',
      datePattern: 'YYYY-MM-DD',
      maxSize: '100m',
      maxFiles: '2d',
      zippedArchive: true,
    }),
  ],
}

export default (moduleName: string): pkg.Logger =>
  createLogger({...options, format: baseFormat, defaultMeta: {class: path.basename(moduleName)}})
