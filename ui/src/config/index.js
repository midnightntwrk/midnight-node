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

import configCommon from './common.json'
import configDevelopment from './development.json'
import configProduction from './production.json'
import configTest from './test.json'
const configEnv = {
  development: configDevelopment,
  production: configProduction,
  test: configTest
}[process.env.NODE_ENV]

// Accepting React env vars and aggregating them into `config` object.
const envVarNames = ['VITE_PROVIDER_SOCKET']
const envVars = envVarNames.reduce((mem, n) => {
  // Remove the `VITE_` prefix
  if (import.meta.env[n] !== undefined) mem[n.slice('VITE_'.length)] = import.meta.env[n]
  return mem
}, {})

const config = { ...configCommon, ...configEnv, ...envVars }
export default config
