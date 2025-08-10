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

import { undeployed } from "@polkadot-api/descriptors";
import { createClient } from "polkadot-api";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { Subscription, take, timeout } from "rxjs";
import { HOST_ADDR } from "./env.js";

export function checkCreate(
  ownerDustHex: string,
  expectedValue: bigint,
): Subscription {
  const client = createClient(
    withPolkadotSdkCompat(getWsProvider(`ws://${HOST_ADDR}:9933`)),
  );
  const api = client.getTypedApi(undeployed);

  const timeId = `createCheck${Math.random()}`;

  console.time(timeId);
  const sub = api.event.NativeTokenObservation.SystemTx.watch((systemTx) => {
    const events = systemTx.body;
    for (const event of events) {
      console.log(`Found event ${event}...`);
      if (event.action.type != "Create") {
        continue;
      }
      if (event.owner.asHex().replace(/^0x/, "") != ownerDustHex) {
        console.log(
          `Found Create event, but owner ${event.owner.asHex()} was not expected owner ${ownerDustHex}`,
        );
        continue;
      }
      if (event.value != expectedValue) {
        console.log(
          `Found Create event with correct owner, but value ${event.value} was not expected value ${expectedValue}`,
        );
        continue;
      }
      return true;
    }

    return false;
  })
    .pipe(timeout(600_000), take(1))
    .subscribe({
      next: () => {
        console.log(
          `Successfully detected create event for owner ${ownerDustHex}, value ${expectedValue}`,
        );
        console.timeEnd(timeId);
        sub.unsubscribe();
        client.destroy();
      },
      error: (err) => {
        console.log(`Error: ${err} while detecting register events`);
        console.timeEnd(timeId);
        sub.unsubscribe();
        client.destroy();
        process.exit(1);
      },
    });

  return sub;
}

export function checkDeregistration(
  cardanoHex: string,
  dustHex: string,
  utxoTxHash: string,
  utxoIndex: number,
): Subscription {
  const client = createClient(
    withPolkadotSdkCompat(getWsProvider(`ws://${HOST_ADDR}:9933`)),
  );
  const api = client.getTypedApi(undeployed);

  const timeId = `createCheck${Math.random()}`;

  console.time(timeId);
  const sub = api.event.NativeTokenObservation.Removed.watch(([, regEntry]) => {
    console.log(`Found dereg entry: ${regEntry}`);

    if (regEntry.cardano_address.asHex().replace(/^0x/, "") != cardanoHex) {
      console.log(
        `Found Dereg event, but cardanoAddress ${regEntry.cardano_address.asHex()} was not expected address ${cardanoHex}`,
      );
      return false;
    }

    if (regEntry.dust_address.asHex().replace(/^0x/, "") != dustHex) {
      console.log(
        `Found Dereg event, but dustAddress ${regEntry.dust_address.asHex()} was not expected address ${dustHex}`,
      );
      return false;
    }

    if (regEntry.utxo_id.asHex().replace(/^0x/, "") != utxoTxHash) {
      console.log(
        `Found Dereg event, but utxoId ${regEntry.utxo_id.asHex()} was not expected ${utxoTxHash}`,
      );
      return false;
    }

    if (regEntry.utxo_index != utxoIndex) {
      console.log(
        `Found Dereg event, but utxoIndex ${regEntry.utxo_index} was not expected ${utxoIndex}`,
      );
      return false;
    }

    return true;
  })
    .pipe(timeout(600_000), take(1))
    .subscribe({
      next: () => {
        console.log(
          `Successfully detected dereg event: ${cardanoHex}, ${dustHex}, ${utxoTxHash}, ${utxoIndex}`,
        );
        console.timeEnd(timeId);
        sub.unsubscribe();
        client.destroy();
      },
      error: (err) => {
        console.log(`Error: ${err} while detecting register events`);
        console.timeEnd(timeId);
        sub.unsubscribe();
        client.destroy();
        process.exit(1);
      },
    });

  return sub;
}

export function checkMultipleReg(
  expectedPairs: { cardanoHex: string; dustHex: string; label?: string }[],
): Subscription {
  const client = createClient(
    withPolkadotSdkCompat(getWsProvider(`ws://${HOST_ADDR}:9933`)),
  );
  const api = client.getTypedApi(undeployed);

  console.log(`Starting registration check for ${expectedPairs.length} pairs`);
  console.time("regCheck");

  const foundPairs = new Set();

  const sub = api.event.NativeTokenObservation.Registered.watch(
    ([cardanoAddress, dustAddresses]) => {
      const cardanoHex = cardanoAddress.asHex().replace(/^0x/, "");
      const dustHex = dustAddresses[0].dust_address.asHex().replace(/^0x/, "");

      // Check if this matches any of our expected pairs
      for (let i = 0; i < expectedPairs.length; i++) {
        const expected = expectedPairs[i];
        if (
          cardanoHex === expected.cardanoHex &&
          dustHex === expected.dustHex
        ) {
          console.log(
            `Found match for ${expected.label || `pair ${i}`}: cardano: ${cardanoHex}, dust: ${dustHex}`,
          );
          foundPairs.add(i);
          return true;
        }
      }

      console.log(
        `No match found for cardano: ${cardanoHex}, dust: ${dustHex}`,
      );
      return false;
    },
  )
    .pipe(
      timeout(600_000),
      take(expectedPairs.length), // Take as many matching events as we have pairs
    )
    .subscribe({
      next: () => {
        console.log(
          `Found registration event (${foundPairs.size}/${expectedPairs.length} complete)`,
        );

        // Check if we've found all pairs
        if (foundPairs.size === expectedPairs.length) {
          console.log(
            `Successfully detected all ${expectedPairs.length} register events`,
          );
          console.timeEnd("regCheck");
          sub.unsubscribe();
          client.destroy();
        }
      },
      error: (err) => {
        console.log(`Error: ${err} while detecting register events`);
        console.timeEnd("regCheck");
        sub.unsubscribe();
        client.destroy();
        process.exit(1);
      },
    });

  return sub;
}
