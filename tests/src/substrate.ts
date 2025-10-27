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

function normHex(x: string) {
  return x.replace(/^0x/i, "").toLowerCase();
}

// export function checkCreate(
//   ownerDustHex: string,
//   expectedValue: bigint,
// ): Subscription {
//   const client = createClient(
//     withPolkadotSdkCompat(getWsProvider(`ws://${HOST_ADDR}:9933`)),
//   );
//   const api = client.getTypedApi(undeployed);
//
//   const timeId = `createCheck${Math.random()}`;
//
//   console.time(timeId);
//   const sub = api.event.NativeTokenObservation.SystemTransactionApplied.watch(
//     () => {
//       // const events = systemTx.body;
//       // for (const event of events) {
//       //   console.log(`Found event ${event}...`);
//       //   if (event.action.type != "Create") {
//       //     continue;
//       //   }
//       //   if (event.owner.asHex().replace(/^0x/, "") != ownerDustHex) {
//       //     console.log(
//       //       `Found Create event, but owner ${event.owner.asHex()} was not expected owner ${ownerDustHex}`,
//       //     );
//       //     continue;
//       //   }
//       //   if (event.value != expectedValue) {
//       //     console.log(
//       //       `Found Create event with correct owner, but value ${event.value} was not expected value ${expectedValue}`,
//       //     );
//       //     continue;
//       //   }
//       //   return true;
//       // }
//
//       // TODO: Re-enable proper checks when tests are re-implemented in Rust
//       // Ledger WASM API doesn't doesn't expose system transaction structure
//       return true;
//     },
//   )
//     .pipe(timeout(600_000), take(1))
//     .subscribe({
//       next: () => {
//         console.log(
//           `Successfully detected create event for owner ${ownerDustHex}, value ${expectedValue}`,
//         );
//         console.timeEnd(timeId);
//         sub.unsubscribe();
//         client.destroy();
//       },
//       error: (err) => {
//         console.log(`Error: ${err} while detecting create events`);
//         console.timeEnd(timeId);
//         sub.unsubscribe();
//         client.destroy();
//         process.exit(1);
//       },
//     });
//
//   return sub;
// }

// export function checkDestroy(
//   ownerDustHex: string,
//   expectedValue: bigint,
// ): Subscription {
//   const client = createClient(
//     withPolkadotSdkCompat(getWsProvider(`ws://${HOST_ADDR}:9933`)),
//   );
//   const api = client.getTypedApi(undeployed);
//
//   const timeId = `createCheck${Math.random()}`;
//
//   console.time(timeId);
//   const sub = api.event.NativeTokenObservation.SystemTransactionApplied.watch(
//     () => {
//       // const events = systemTx.body;
//       // for (const event of events) {
//       //   console.log(`Found event ${event}...`);
//       //   if (event.action.type != "Destroy") {
//       //     continue;
//       //   }
//       //   if (event.owner.asHex().replace(/^0x/, "") != ownerDustHex) {
//       //     console.log(
//       //       `Found Create event, but owner ${event.owner.asHex()} was not expected owner ${ownerDustHex}`,
//       //     );
//       //     continue;
//       //   }
//       //   if (event.value != expectedValue) {
//       //     console.log(
//       //       `Found Create event with correct owner, but value ${event.value} was not expected value ${expectedValue}`,
//       //     );
//       //     continue;
//       //   }
//       //   return true;
//       // }
//
//       // TODO: Re-enable proper checks when tests are re-implemented in Rust
//       // Ledger WASM API doesn't doesn't expose system transaction structure
//       return false;
//     },
//   )
//     .pipe(timeout(600_000), take(1))
//     .subscribe({
//       next: () => {
//         console.log(
//           `Successfully detected destroy event for owner ${ownerDustHex}, value ${expectedValue}`,
//         );
//         console.timeEnd(timeId);
//         sub.unsubscribe();
//         client.destroy();
//       },
//       error: (err) => {
//         console.log(`Error: ${err} while detecting destroy events`);
//         console.timeEnd(timeId);
//         sub.unsubscribe();
//         client.destroy();
//         process.exit(1);
//       },
//     });
//
//   return sub;
// }

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

  const sub = api.event.CNightObservation.Deregistration.watch((reg) => {
    if (reg.cardano_address.asHex().replace(/^0x/, "") != cardanoHex) {
      console.log(
        `Found Dereg event, but cardanoAddress ${reg.cardano_address.asHex()} was not expected address ${cardanoHex}`,
      );
      return false;
    }

    if (reg.dust_address.asHex().replace(/^0x/, "") != dustHex) {
      console.log(
        `Found Dereg event, but dustAddress ${reg.dust_address.asHex()} was not expected address ${dustHex}`,
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
        console.log(`Error: ${err} while detecting deregister events`);
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

  const sub = api.event.CNightObservation.Registration.watch((reg) => {
    const cardanoHex = reg.cardano_address.asHex().replace(/^0x/, "");
    const dustHex = reg.dust_address.asHex().replace(/^0x/, "");

    // Check if this matches any of our expected pairs
    for (let i = 0; i < expectedPairs.length; i++) {
      const expected = expectedPairs[i];
      if (cardanoHex === expected.cardanoHex && dustHex === expected.dustHex) {
        console.log(
          `Found match for ${expected.label || `pair ${i}`}: cardano: ${cardanoHex}, dust: ${dustHex}`,
        );
        foundPairs.add(i);
        return true;
      }
    }

    console.log(`No match found for cardano: ${cardanoHex}, dust: ${dustHex}`);
    return false;
  })
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
        console.log(`Error: ${err} while detecting registration events`);
        console.timeEnd("regCheck");
        sub.unsubscribe();
        client.destroy();
        process.exit(1);
      },
    });

  return sub;
}

export function checkMappingAdded(
  cardanoHex: string,
  dustHex: string,
  utxoTxHash: string,
): Subscription {
  const client = createClient(
    withPolkadotSdkCompat(getWsProvider(`ws://${HOST_ADDR}:9933`)),
  );
  const api = client.getTypedApi(undeployed);

  const timeId = `mappingAddedCheck${Math.random()}`;
  console.time(timeId);

  const sub = api.event.CNightObservation.MappingAdded.watch((m) => {
    const cardano = normHex(m.cardano_address.asHex());
    const dust = normHex(m.dust_address);
    const utxo = normHex(m.utxo_id);

    if (cardano !== normHex(cardanoHex)) {
      console.log(`MappingAdded: cardano mismatch ${cardano} != ${cardanoHex}`);
      return false;
    }
    if (dust !== normHex(dustHex)) {
      console.log(`MappingAdded: dust mismatch ${dust} != ${dustHex}`);
      return false;
    }
    if (utxo !== normHex(utxoTxHash)) {
      console.log(`MappingAdded: utxo mismatch ${utxo} != ${utxoTxHash}`);
      return false;
    }
    return true;
  })
    .pipe(timeout(600_000), take(1))
    .subscribe({
      next: () => {
        console.log(
          `Successfully detected MappingAdded for cardano=${cardanoHex}, dust=${dustHex}, utxo=${utxoTxHash}`,
        );
        console.timeEnd(timeId);
        sub.unsubscribe();
        client.destroy();
      },
      error: (err) => {
        console.log(`Error: ${err} while detecting MappingAdded`);
        console.timeEnd(timeId);
        sub.unsubscribe();
        client.destroy();
        process.exit(1);
      },
    });

  return sub;
}

export function checkMappingRemoved(
  cardanoHex: string,
  dustHex: string,
  utxoTxHash: string,
): Subscription {
  const client = createClient(
    withPolkadotSdkCompat(getWsProvider(`ws://${HOST_ADDR}:9933`)),
  );
  const api = client.getTypedApi(undeployed);

  const timeId = `mappingRemovedCheck${Math.random()}`;
  console.time(timeId);

  const sub = api.event.CNightObservation.MappingRemoved.watch((m) => {
    const cardano = normHex(m.cardano_address.asHex());
    const dust = normHex(m.dust_address);
    const utxo = normHex(m.utxo_id);

    if (cardano !== normHex(cardanoHex)) {
      console.log(
        `MappingRemoved: cardano mismatch ${cardano} != ${cardanoHex}`,
      );
      return false;
    }
    if (dust !== normHex(dustHex)) {
      console.log(`MappingRemoved: dust mismatch ${dust} != ${dustHex}`);
      return false;
    }
    if (utxo !== normHex(utxoTxHash)) {
      console.log(`MappingRemoved: utxo mismatch ${utxo} != ${utxoTxHash}`);
      return false;
    }
    return true;
  })
    .pipe(timeout(600_000), take(1))
    .subscribe({
      next: () => {
        console.log(
          `Successfully detected MappingRemoved for cardano=${cardanoHex}, dust=${dustHex}, utxo=${utxoTxHash}`,
        );
        console.timeEnd(timeId);
        sub.unsubscribe();
        client.destroy();
      },
      error: (err) => {
        console.log(`Error: ${err} while detecting MappingRemoved`);
        console.timeEnd(timeId);
        sub.unsubscribe();
        client.destroy();
        process.exit(1);
      },
    });

  return sub;
}
