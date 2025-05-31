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
