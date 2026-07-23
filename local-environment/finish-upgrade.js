// Complete the governance runtime upgrade against the live (22000) runtime
// signature: motionClose(motion_hash) — 1 arg — then applyAuthorizedUpgrade.
const fs = require("fs");
const { ApiPromise, WsProvider, Keyring } = require("@polkadot/api");
const { blake2AsHex } = require("@polkadot/util-crypto");
const { u8aToHex } = require("@polkadot/util");

const WASM = "artifacts/upgrade/midnight_node_runtime.compact.compressed.wasm";

function signAndWait(tx, signer, label) {
  return new Promise((resolve, reject) => {
    tx.signAndSend(signer, { nonce: -1 }, (result) => {
      if (result.dispatchError) {
        let msg = result.dispatchError.toString();
        if (result.dispatchError.isModule) {
          const mod = tx.registry.findMetaError(result.dispatchError.asModule);
          msg = `${mod.section}.${mod.name}: ${mod.docs.join(" ")}`;
        }
        reject(new Error(`${label} dispatch error: ${msg}`));
      } else if (result.status.isInBlock) {
        console.log(`${label} in block ${result.status.asInBlock.toHex()}`);
        const events = result.events.map((e) => `${e.event.section}.${e.event.method}`);
        console.log(`  events: ${events.join(", ")}`);
        resolve(result);
      } else if (result.isError) {
        reject(new Error(`${label} failed: ${result.status.toString()}`));
      }
    }).catch(reject);
  });
}

(async () => {
  const api = await ApiPromise.create({
    provider: new WsProvider("ws://127.0.0.1:9950"),
    noInitWarn: true,
  });
  const keyring = new Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");

  const wasm = fs.readFileSync(WASM);
  const wasmHex = u8aToHex(wasm);
  const codeHash = blake2AsHex(wasm, 256);
  console.log(`wasm: ${wasm.length} bytes, hash ${codeHash}`);

  const authorizeCall = api.tx.system.authorizeUpgrade(codeHash);
  const motionHash = blake2AsHex(authorizeCall.method.toU8a());
  console.log(`motion hash: ${motionHash}`);

  const nArgs = api.tx.federatedAuthority.motionClose.meta.args.length;
  console.log(`on-chain motionClose arity: ${nArgs}`);
  const closeTx =
    nArgs === 1
      ? api.tx.federatedAuthority.motionClose(motionHash)
      : api.tx.federatedAuthority.motionClose(
          motionHash,
          api.createType("WeightV2", { refTime: 10_000_000_000, proofSize: 65_536 }),
        );
  await signAndWait(closeTx, alice, "federatedAuthority.motionClose");

  const applyRes = await signAndWait(
    api.tx.system.applyAuthorizedUpgrade(wasmHex),
    alice,
    "system.applyAuthorizedUpgrade",
  );
  const hasCodeUpdated = applyRes.events.some(
    (e) => e.event.section === "system" && e.event.method === "CodeUpdated",
  );
  console.log(hasCodeUpdated ? "SUCCESS: system.CodeUpdated emitted" : "WARNING: no CodeUpdated event");
  process.exit(hasCodeUpdated ? 0 : 2);
})().catch((e) => {
  console.error("FAILED:", e.message);
  process.exit(1);
});
