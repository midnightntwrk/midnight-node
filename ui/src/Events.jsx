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
import { Feed, Grid, Button } from 'semantic-ui-react'

import { useSubstrateState } from './substrate-lib'

// Events to be filtered from feed
const FILTERED_EVENTS = [
  'system:ExtrinsicSuccess::(phase={"applyExtrinsic":0})',
]

const eventName = ev => `${ev.section}:${ev.method}`
const eventParams = ev => JSON.stringify(ev.data)

function Main(props) {
  const { api } = useSubstrateState()
  const [eventFeed, setEventFeed] = useState([])

  useEffect(() => {
    let unsub = null
    const allEvents = async () => {
      unsub = await api.query.system.events(events => {
        // loop through the Vec<EventRecord>
        events.forEach(record => {
          // extract the phase, event and the event types
          const { event, phase } = record

          // show what we are busy with
          const evHuman = event.toHuman()
          const evName = eventName(evHuman)
          const evParams = eventParams(evHuman)
          const evNamePhase = `${evName}::(phase=${phase.toString()})`

          if (FILTERED_EVENTS.includes(evNamePhase)) return

          setEventFeed(e => [
            {
              key: e.length,
              icon: 'bell',
              summary: evName,
              content: evParams,
            },
            ...e,
          ])
        })
      })
    }

    allEvents()
    return () => unsub && unsub()
  }, [api.query.system])

  const { feedMaxHeight = 250 } = props

  return (
    <Grid.Column width={8}>
      <h1 style={{ float: 'left' }}>Events</h1>
      <Button
        basic
        circular
        size="mini"
        color="grey"
        floated="right"
        icon="erase"
        onClick={_ => setEventFeed([])}
      />
      <Feed
        style={{ clear: 'both', overflow: 'auto', maxHeight: feedMaxHeight }}
        events={eventFeed}
      />
    </Grid.Column>
  )
}

export default function Events(props) {
  const { api } = useSubstrateState()
  return api.query && api.query.system && api.query.system.events ? (
    <Main {...props} />
  ) : null
}
