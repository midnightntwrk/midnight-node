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

import { readFile } from "fs/promises";
import {
  validatorToScriptHash,
  Script,
  applyDoubleCborEncoding,
  applyParamsToScript,
  fromText,
} from "@lucid-evolution/lucid";
import { decode as decodeCbor } from "cbor2";
import { bech32 } from "bech32";
import assert from "assert";

import blueprint from "./plutus.json" with { type: "json" };

const redemptionSkeleton = blueprint.validators.find(
  (v) => v.title === "redemption.redemption.spend",
);
assert(
  redemptionSkeleton,
  "Failed for find redemption.redemption.spend in plutus.json",
);

const ownerKeyFilePayment = await readFile(
  "../local-environment/src/networks/local-env/configurations/cardano/keys/owner-utxo.skey",
  "utf-8",
);
const ownerKeyJsonPayment = JSON.parse(ownerKeyFilePayment);
export const ownerKeyPaymentCbor = ownerKeyJsonPayment["cborHex"] as string;

const bytes: Uint8Array = decodeCbor(ownerKeyPaymentCbor);
const words = bech32.toWords(bytes);
export const ownerKeyPaymentBech32 = bech32.encode("ed25519_sk", words);

const ownerKeyFileStake = await readFile(
  "../local-environment/src/networks/local-env/configurations/cardano/keys/owner-stake.vkey",
  "utf-8",
);
const ownerKeyJsonStake = JSON.parse(ownerKeyFileStake);
export const ownerKeyStakeCbor = ownerKeyJsonStake["cborHex"] as string;

const authTokenPolicyFile = await readFile(
  "../scripts/cnight-generates-dust/auth_token_policy.plutus",
  "utf-8",
);
const authTokenPolicyJson = JSON.parse(authTokenPolicyFile);
export const authTokenScript: Script = {
  type: "PlutusV2",
  script: authTokenPolicyJson["cborHex"],
};
export const authTokenPolicyId = validatorToScriptHash(authTokenScript);

const cnightPolicyFile = await readFile(
  "../scripts/cnight-generates-dust/cnight_policy.plutus",
  "utf-8",
);
const cnightPolicyJson = JSON.parse(cnightPolicyFile);
export const cnightScript: Script = {
  type: "PlutusV2",
  script: cnightPolicyJson["cborHex"],
};
export const cnightPolicyId = validatorToScriptHash(cnightScript);

const mappingValidatorPolicyFile = await readFile(
  "../scripts/cnight-generates-dust/mapping_validator.plutus",
  "utf-8",
);
const mappingValidatorJSON = JSON.parse(mappingValidatorPolicyFile);
export const mappingValidatorScript: Script = {
  type: "PlutusV2",
  script: mappingValidatorJSON["cborHex"],
};
export const mappingValidatorPolicyId = validatorToScriptHash(
  mappingValidatorScript,
);

export const redemptionScript: Script = {
  type: "PlutusV3",
  script: applyDoubleCborEncoding(
    applyParamsToScript(redemptionSkeleton.compiledCode, [
      cnightPolicyId,
      fromText(""),
    ]),
  ),
};

export const redemptionPolicyId = validatorToScriptHash(redemptionScript);
