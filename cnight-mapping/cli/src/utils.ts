import { Lucid, LucidEvolution, makeWalletFromPrivateKey, Network } from "@lucid-evolution/lucid";
import { readFile } from "fs/promises";
import { decode as decodeCbor } from "cbor2";
import { bech32 } from "bech32";

// Helper function to convert to JSON for logging
export const toJson = (obj: object): string => {
  return JSON.stringify(obj, (_key, value) => (typeof value === 'bigint' ? value.toString() + 'n' : value), 2);
};

export function assertNetwork(value: string): asserts value is Network {
  if (!["Mainnet", "Preview", "Preprod", "Custom"].includes(value)) {
    throw new Error(`Invalid network: ${value}`);
  }
}

export async function setWalletFromKeyFile(lucid: LucidEvolution, filename: string) {
  const ownerKeyFilePayment = await readFile(filename, "utf-8");
  const ownerKeyJsonPayment = JSON.parse(ownerKeyFilePayment);
  const ownerKeyPaymentCbor = ownerKeyJsonPayment["cborHex"] as string;

  const bytes: Uint8Array = decodeCbor(ownerKeyPaymentCbor);
  const words = bech32.toWords(bytes);
  const privateKey = bech32.encode("ed25519_sk", words);

  lucid.selectWallet.fromPrivateKey(privateKey);
}
