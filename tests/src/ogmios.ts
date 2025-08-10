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

import {
  createInteractionContext,
  createLedgerStateQueryClient,
} from "@cardano-ogmios/client";
import { HOST_ADDR, OGMIOS_PORT } from "./env.js";

export async function createContext() {
  return await createInteractionContext(
    (err) => console.error(err),
    () => console.log("Connection closed."),
    { connection: { host: HOST_ADDR, port: OGMIOS_PORT } },
  );
}

export async function createClient() {
  const context = await createContext();
  const client = await createLedgerStateQueryClient(context);
  return client;
}
