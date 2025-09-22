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

// This component will simply add utility functions to your developer console.
import { useSubstrateState } from '..'
import * as util from '@polkadot/util'
import * as utilCrypto from '@polkadot/util-crypto'

export default function DeveloperConsole(props) {
  const { api, apiState, keyring, keyringState } = useSubstrateState()
  if (apiState === 'READY') {
    window.api = api
  }
  if (keyringState === 'READY') {
    window.keyring = keyring
  }
  window.util = util
  window.utilCrypto = utilCrypto

  return null
}
