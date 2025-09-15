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
  Lucid,
  Kupmios,
  makeWalletFromPrivateKey,
  Wallet,
  LucidEvolution,
  UTxO,
  Assets,
  validatorToAddress,
  Data,
  Constr,
  OutputDatum,
  getAddressDetails,
  Credential,
  TxSigned,
  addressFromHexOrBech32,
} from "@lucid-evolution/lucid";
import {
  authTokenPolicyId,
  authTokenScript,
  cnightPolicyId,
  cnightScript,
  mappingValidatorScript,
  redemptionScript,
} from "./static.js";
import { createLedgerStateQueryClient } from "@cardano-ogmios/client";
import { createContext } from "./ogmios.js";
import { Buffer } from "buffer";

import { bech32, bech32m } from "bech32";

export function stringToUtf8Hex(input: string): string {
  const encoder = new TextEncoder();
  return Buffer.from(encoder.encode(input)).toString("hex");
}

export function bech32ToHex(input: string): string {
  const decoded = bech32.decode(input);
  const bytes = bech32.fromWords(decoded.words);
  return Buffer.from(new Uint8Array(bytes)).toString("hex");
}

export function bech32mToHex(input: string): string {
  const decoded = bech32m.decode(input, 200);
  const bytes = bech32m.fromWords(decoded.words);
  return Buffer.from(new Uint8Array(bytes)).toString("hex");
}

const CredentialSchema = Data.Tuple([Data.Bytes()]);

const AddressSchema = Data.Tuple([
  CredentialSchema, // payment_credential
  Data.Nullable(CredentialSchema), // stake_credential
]);

const RedemptionDatumSchema = Data.Tuple([
  AddressSchema, // destination_address
  Data.Integer(), // increment_amount_quotient
  Data.Integer(), // next_thaw
  Data.Integer(), // increments_left
  Data.Integer(), // redemption_increment_period
]);

// Define the TypeScript type alias from the schema
type RedemptionDatum = Data.Static<typeof RedemptionDatumSchema>;
const RedemptionDatum = RedemptionDatumSchema as unknown as RedemptionDatum;

interface LucidEvoOptions {
  hostAddr: string;
  privateKeyBech32: string;
  dryRun?: boolean;
}

export class LucidEvoTest {
  kupmios: Kupmios;
  wallet: Wallet;
  privateKey: string;
  lucidState: LucidEvolution | null = null;
  dryRun: boolean;

  constructor({ hostAddr, privateKeyBech32, dryRun = false }: LucidEvoOptions) {
    this.privateKey = privateKeyBech32;
    this.kupmios = new Kupmios(
      `http://${hostAddr}:1442`,
      `http://${hostAddr}:1337`,
    );
    this.wallet = makeWalletFromPrivateKey(
      this.kupmios,
      "Custom",
      this.privateKey,
    );
    this.dryRun = dryRun ?? false;
  }

  selectWallet(privateKey: string) {
    this.privateKey = privateKey;
    this.wallet = makeWalletFromPrivateKey(this.kupmios, "Custom", privateKey);
  }

  async waitForNextCardanoBlock() {
    if (this.dryRun) {
      console.log("DRY RUN: Not waiting for Cardano");
      return;
    }
    const context = await createContext();
    const client = await createLedgerStateQueryClient(context);

    const startHeight = await client.networkBlockHeight();
    for (;;) {
      // Sleep
      await new Promise((resolve) => setTimeout(resolve, 500));

      const newHeight = await client.networkBlockHeight();
      if (startHeight != newHeight) {
        console.log(`new Cardano slot: ${newHeight}`);
        client.shutdown();
        return;
      }
    }
  }

  async submit(tx: TxSigned) {
    if (this.dryRun) {
      console.log("DRY RUN: tx not submitted");
    } else {
      const txHash = await tx.submit();
      console.log(`tx sent successfully: ${txHash}`);
    }
  }

  async getLucid() {
    if (this.lucidState) {
      return this.lucidState;
    }
    const lucid = await Lucid(this.kupmios, "Custom");
    lucid.selectWallet.fromPrivateKey(this.privateKey);
    return lucid;
  }

  async printUtxos(addressOrCredential?: string | Credential) {
    const utxos = await this.lucidState?.utxosAt(
      addressOrCredential ?? (await this.wallet.address()),
    );
    console.log(utxos);
  }

  async printAddresses() {
    console.log(await this.wallet.address());
  }

  async makeCollateral(
    destAddress: string,
    inputState?: LucidEvolution,
  ): Promise<UTxO> {
    const lucid = inputState ?? (await this.getLucid());
    const [newWalletUTxOs, contractOutputs, txSignBuilder] = await lucid
      .newTx()
      .pay.ToAddress(destAddress, { lovelace: 5_000_000n })
      .chain();

    const signedTx = await txSignBuilder.sign.withWallet().complete();
    await this.submit(signedTx);

    lucid.overrideUTxOs(newWalletUTxOs);

    const collateralUtxo = contractOutputs.find(
      (u) => u.address == destAddress,
    );
    if (!collateralUtxo) {
      throw new Error(`No UTXO found for address ${destAddress}`);
    }

    console.log(`collateralUtxo: ${collateralUtxo}`);

    this.lucidState = lucid;
    return collateralUtxo;
  }

  async mintCnight(
    destAddress: string,
    amount: bigint,
    inputState?: LucidEvolution,
  ): Promise<UTxO> {
    const lucid = inputState ?? (await this.getLucid());

    const emptyRedeemer = Data.to(new Constr(0, []));
    const [newWalletUTxOs, contractOutputs, txSignBuilder] = await lucid
      .newTx()

      .pay.ToAddress(destAddress, { [cnightPolicyId]: amount })
      .mintAssets({ [cnightPolicyId]: amount }, emptyRedeemer)
      .attach.MintingPolicy(cnightScript)
      .chain();

    const signedTx = await txSignBuilder.sign.withWallet().complete();
    await this.submit(signedTx);

    lucid.overrideUTxOs(newWalletUTxOs);
    const cnightUtxo = contractOutputs.find((u) => u.address == destAddress);
    if (!cnightUtxo) {
      throw new Error(`No UTXO found for address ${destAddress}`);
    }

    this.lucidState = lucid;
    return cnightUtxo;
  }

  async send(
    destAddress: string,
    assets: Assets,
    inputState?: LucidEvolution,
  ): Promise<UTxO[]> {
    const lucid = inputState ?? (await this.getLucid());

    const [newWalletUTxOs, outputs, txSignBuilder] = await lucid
      .newTx()
      .pay.ToAddress(destAddress, assets)
      .chain();

    const signedTx = await txSignBuilder.sign.withWallet().complete();
    await this.submit(signedTx);

    lucid.overrideUTxOs(newWalletUTxOs);

    this.lucidState = lucid;
    return outputs;
  }

  async register(
    cardanoAddress: string,
    midnightAddressHex: string,
    inputState?: LucidEvolution,
  ): Promise<UTxO[]> {
    const lucid = inputState ?? (await this.getLucid());

    const cardanoAddressHex = bech32ToHex(cardanoAddress);
    // let midnightAddressHex = bech32mToHex(midnightAddress);
    // // First 32 bytes == coin public key
    // // Spec: https://github.com/midnightntwrk/midnight-architecture/blob/main/components/WalletEngine/Specification.md#shielded-payment-address
    // midnightAddressHex = midnightAddressHex.slice(0, 64);

    const datumValue = Data.to(
      new Constr(0, [cardanoAddressHex, midnightAddressHex]),
    );

    const datum: OutputDatum = {
      kind: "inline",
      value: datumValue,
    };

    const mintRedeemer = Data.to(new Constr(0, []));

    const validatorAddress = validatorToAddress(
      "Custom",
      mappingValidatorScript,
    );
    console.log(`validator address: ${validatorAddress}`);

    const [newWalletUTxOs, outputs, txSignBuilder] = await lucid
      .newTx()
      .pay.ToContract(validatorAddress, datum, {
        [authTokenPolicyId]: 1n,
        lovelace: 150_000_000n,
      })
      .mintAssets({ [authTokenPolicyId]: 1n }, mintRedeemer)
      .attach.MintingPolicy(authTokenScript)
      .chain();

    const signedTx = await txSignBuilder.sign.withWallet().complete();
    await this.submit(signedTx);
    lucid.overrideUTxOs(newWalletUTxOs);

    this.lucidState = lucid;
    return outputs;
  }

  async deregister(
    registration: UTxO,
    inputState?: LucidEvolution,
  ): Promise<UTxO[]> {
    const lucid = inputState ?? (await this.getLucid());

    const emptyRedeemer = Data.to(new Constr(0, []));
    const [newWalletUTxOs, outputs, txSignBuilder] = await lucid
      .newTx()
      .collectFrom([registration], emptyRedeemer)
      .pay.ToAddress(await this.wallet.address(), { lovelace: 20_000_000n })
      .attach.SpendingValidator(mappingValidatorScript)
      .mintAssets({ [authTokenPolicyId]: -1n }, emptyRedeemer)
      .attach.MintingPolicy(authTokenScript)
      .chain();

    const signedTx = await txSignBuilder.sign.withWallet().complete();
    await this.submit(signedTx);
    lucid.overrideUTxOs(newWalletUTxOs);

    this.lucidState = lucid;
    return outputs;
  }

  async createRedemptionContract(
    destinationAddress: string,
    incrementAmount: number,
    increments: number,
    inputState?: LucidEvolution,
    collectFromUtxos?: UTxO[],
  ): Promise<UTxO[]> {
    const lucid = inputState ?? (await this.getLucid());

    const validatorAddress = validatorToAddress("Custom", redemptionScript);

    console.log(`destinationAddress (bech32): ${destinationAddress}`);
    console.log(`destinationAddress (hex): ${bech32ToHex(destinationAddress)}`);

    const destinationAddressDetails = getAddressDetails(destinationAddress);
    if (!destinationAddressDetails.paymentCredential) {
      throw new Error(
        `Invalid address: The destination address must be a Shelley-era payment address (starting with 'addr'), not a Byron or stake address.`,
      );
    }

    const datumValue = Data.to(
      new Constr(0, [
        bech32ToHex(destinationAddress),
        BigInt(incrementAmount),
        BigInt(0),
        BigInt(increments),
        BigInt(0),
      ]),
    );

    const datum: OutputDatum = {
      kind: "inline",
      value: datumValue,
    };

    let txBuilder = lucid.newTx();

    if (collectFromUtxos) {
      txBuilder = txBuilder.collectFrom(collectFromUtxos);
    }

    const [newWalletUTxOs, outputs, txSignBuilder] = await txBuilder.pay
      .ToContract(validatorAddress, datum, {
        [cnightPolicyId]: BigInt(incrementAmount * increments),
      })
      .addSigner(destinationAddress)
      .chain();

    const signedTx = await txSignBuilder.sign
      .withWallet()
      .sign.withPrivateKey(this.privateKey)
      .complete();
    console.log(signedTx);
    await this.submit(signedTx);
    lucid.overrideUTxOs(newWalletUTxOs);

    this.lucidState = lucid;
    return outputs;
  }

  async executeRedemption(
    redemption: UTxO,
    inputState?: LucidEvolution,
  ): Promise<UTxO[]> {
    // Changed to void as it's performing an action
    const lucid = inputState ?? (await this.getLucid());

    if (!redemption.datum) {
      throw new Error("UTxO does not contain an inline datum.");
    }

    // Decode the raw datum (it should be a Constr object)
    const datumConstr = Data.from(redemption.datum);
    console.log("Raw datum:", datumConstr);

    // Extract fields from Constructor 0
    if (!(datumConstr instanceof Constr) || datumConstr.index !== 0) {
      throw new Error("Invalid datum structure - expected Constructor 0");
    }

    const [
      destinationAddressBytes,
      incrementAmount,
      nextThaw,
      incrementsLeft,
      redemptionIncrementPeriod,
    ] = datumConstr.fields;

    console.log("Decoded Datum:");
    console.log("  Destination Address (hex):", destinationAddressBytes);
    console.log("  Increment Amount:", incrementAmount.toString());
    console.log("  Next Thaw:", nextThaw.toString());
    console.log("  Increments Left:", incrementsLeft.toString());
    console.log("  Redemption Period:", redemptionIncrementPeriod.toString());

    const destinationAddress = addressFromHexOrBech32(
      destinationAddressBytes as string,
    ).to_bech32();
    // Convert address bytes back to bech32 if needed
    // const destinationAddress = toBech32(fromHex(destinationAddressBytes as string));
    console.log("  Destination Address (bech32):", destinationAddress);

    const validatorAddress = validatorToAddress("Custom", redemptionScript);

    const datumValue = Data.to(
      new Constr(0, [
        bech32ToHex(destinationAddress),
        incrementAmount as bigint,
        BigInt(0),
        (incrementsLeft as bigint) - 1n,
        BigInt(0),
      ]),
    );

    const datum: OutputDatum = {
      kind: "inline",
      value: datumValue,
    };

    const emptyRedeemer = Data.to(new Constr(0, []));

    let txBuilder = lucid
      .newTx()
      .collectFrom([redemption], emptyRedeemer)
      .pay.ToAddress(destinationAddress, {
        [cnightPolicyId]: incrementAmount as bigint,
      })
      .attach.SpendingValidator(redemptionScript);

    if ((incrementsLeft as bigint) > 1n) {
      txBuilder = txBuilder.pay.ToContract(validatorAddress, datum, {
        [cnightPolicyId]:
          (incrementAmount as bigint) * ((incrementsLeft as bigint) - 1n),
      });
    }

    const [newWalletUTxOs, outputs, txSignBuilder] = await txBuilder.chain();

    const signedTx = await txSignBuilder.sign.withWallet().complete();
    await this.submit(signedTx);
    lucid.overrideUTxOs(newWalletUTxOs);

    this.lucidState = lucid;
    return outputs;
  }
}
