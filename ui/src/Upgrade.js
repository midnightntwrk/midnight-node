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

import React, { useState } from 'react'
import { Form, Input, Grid } from 'semantic-ui-react'
import { TxButton } from './substrate-lib/components'

export default function Main(props) {
  const [status, setStatus] = useState('')
  const [proposal, setProposal] = useState({})

  const bufferToHex = buffer => {
    return Array.from(new Uint8Array(buffer))
      .map(b => b.toString(16).padStart(2, '0'))
      .join('')
  }

  const handleFileChosen = file => {
    const fileReader = new FileReader()
    fileReader.onloadend = e => {
      const content = bufferToHex(fileReader.result)
      setProposal(`0x${content}`)
    }

    fileReader.readAsArrayBuffer(file)
  }

  return (
    <Grid.Column width={8}>
      <h1>Upgrade Runtime</h1>
      <Form>
        <Form.Field>
          <Input
            type="file"
            id="file"
            label="Wasm File"
            accept=".wasm"
            onChange={e => handleFileChosen(e.target.files[0])}
          />
        </Form.Field>
        <Form.Field style={{ textAlign: 'center' }}>
          <TxButton
            label="Upgrade"
            type="UNCHECKED-SUDO-TX"
            setStatus={setStatus}
            attrs={{
              palletRpc: 'system',
              callable: 'setCode',
              inputParams: [proposal],
              paramFields: [true],
            }}
          />
        </Form.Field>
        <div style={{ overflowWrap: 'break-word' }}>{status}</div>
      </Form>
    </Grid.Column>
  )
}
