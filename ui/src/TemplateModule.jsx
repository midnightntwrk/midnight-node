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

import React, { useEffect, useState } from 'react'
import { Grid } from 'semantic-ui-react'

import { useSubstrateState } from './substrate-lib'
import {stringify} from "@polkadot/util";

function Main(props) {
  const { api } = useSubstrateState()

  // The transaction submission status

  // The currently stored value
  const [currentValue, setCurrentValue] = useState(0)

  useEffect(() => {
    let unsubscribe
    api.query.mnModule
      .state(newValue => {
        // The storage value is an Option<u32>
        // So we have to check whether it is None first
        // There is also unwrapOr
        if (newValue.isNone) {
          setCurrentValue('<None>')
        } else {
          setCurrentValue(stringify(newValue))
        }
      })
      .then(unsub => {
        unsubscribe = unsub
      })
      .catch(console.error)

    return () => unsubscribe && unsubscribe()
  }, [api.query.mnModule])

  return (
    <Grid.Column width={8}>
      <h1>RAW midnight state</h1>
      <textarea value={currentValue} contentEditable={"false"}></textarea>
    </Grid.Column>
  )
}

export default function TemplateModule(props) {
  const { api } = useSubstrateState()
  return api.query.mnModule && api.query.mnModule.state ? (
    <Main {...props} />
  ) : null
}
