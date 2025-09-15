# Devnet Chain Initialization Log

release: node-0.13.5-rc.1

date: 2025-08-07T11:30:17.300645+02:00

## Keys preparation
I'll use the same keys as in devnet (node 1-6).

Let's first generate new keys to create required directory structure by the wizard.
```
./target/release/midnight-node wizards generate-keys
```

Now, let's replace the seeds in the generated files `./data/keystore/*` and `./data/network/secret_ed25519` with devnet values for node 1.

Done.

We can proceed with the wizard to prepare the configuration.


## Configuration Preparation
To prepare configuration we need ogmios access. I'm gonna use devnet ogmios.

Additionally, we need to provide the payment signing key file path, which is `res/devnet/governance.skey` in this case. I copied it from `midnight-node-ops` repo.

```
./target/release/midnight-node wizards prepare-configuration
This 🧙 wizard will generate chain config file
> node base path ./data
> Your bootnode should be accessible via: hostname
> Enter bootnode TCP port 30333
> Enter bootnode hostname devnet
Bootnode saved successfully. Keep in mind that you can manually modify pc-chain-config.json, to edit bootnodes.
Now, let's set up the genesis UTXO. It identifies the partner chain. This wizard will query Ogmios for your UTXOs using the address derived from the payment signing key. This signing key will be then used for spending the genesis UTXO in order to initialize the chain governance. Please provide required data.
> Ogmios protocol (http/https) https
> Ogmios hostname ogmios.devnet.midnight.network
> Ogmios port 443
> path to the payment signing key file res/devnet/governance.skey
⚙️ Querying UTXOs of addr_test1vrdc9rry8j3d5v73aw0sks3l8uy4xsc0nstdnmp462q0ljshcfl6x from Ogmios at https://ogmios.devnet.midnight.network:443...
> Select an UTXO to use as the genesis UTXO d8eca39a90a74d1da650581ed6269aaecc2cde769e6023fd5a18706b18bca4d7#1 (9966971374 lovelace)
Please provide the initial chain governance key hashes:
> Space separated keys hashes of the initial Multisig Governance Authorities 0xdb828c643ca2da33d1eb9f0b423f3f0953430f9c16d9ec35d280ffca
> Initial Multisig Governance Threshold 1
> Governance will be initialized with:
Governance authorities:
        0xdb828c643ca2da33d1eb9f0b423f3f0953430f9c16d9ec35d280ffca
Threshold: 1
Do you want to continue? Yes
2025-08-07T11:32:28.539176+02:00 INFO partner_chains_cardano_offchain::init_governance - ✉️ Submitter address: addr_test1vrdc9rry8j3d5v73aw0sks3l8uy4xsc0nstdnmp462q0ljshcfl6x
2025-08-07T11:32:28.539246+02:00 INFO partner_chains_cardano_offchain::init_governance - 💱 1 UTXOs available
2025-08-07T11:32:28.966324+02:00 INFO partner_chains_cardano_offchain::init_governance - ✅ Transaction submitted. ID: 2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47
2025-08-07T11:32:28.966340+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:32:34.023212+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:32:39.116839+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:32:44.180359+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:32:49.242002+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:32:54.301705+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:32:59.363046+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:04.431474+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:09.491012+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:14.554240+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:19.616487+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:24.680921+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:29.741639+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:34.802415+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:39.861295+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:44.928949+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:49.999867+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:33:55.136473+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:34:00.199133+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:34:05.265319+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:34:10.331838+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:34:15.401432+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:34:20.465820+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:34:25.524941+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:34:30.587636+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:34:35.651465+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:34:40.709781+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47#0'
2025-08-07T11:34:40.782493+02:00 INFO partner_chains_cardano_offchain::await_tx - Transaction output '2d9f361aaf01deebabf2bb18e2d02784a6838ee3cc85165de0112f92deb08f47'
Governance initialized successfully for UTXO: d8eca39a90a74d1da650581ed6269aaecc2cde769e6023fd5a18706b18bca4d7#1
Cardano addresses have been set up:
- Committee Candidates Address: addr_test1wp6srw86tvcms65y6geg4qmgnwvpkd20evs0zm0pfdtv8hqadv3l9
- D Parameter Policy ID: ba0d04dd197bf3a663c487cd0360c0c6a496dcf8bd52e73655319695
- Permissioned Candidates Policy ID: f1206a4ddef8807d8aac05faf058bdfe960891be8f80f02027559aaf
- Illiquid Supply Address: addr_test1wrr8e7s5pqv0zc3zwhd6v4tnfvkmwscvqgpnxauqfd5yj8qktalge
- Governed Map Validator Address: addr_test1wpdrv3yq2me42ud8htn23mp824pnxgk7cq2a9l5z3tj4ldq96v8y5
- Governed Map Policy Id: ec4ff2e365a5cc1552cdbb897df622170f251eeeaeb7360745f31f31
Partner Chains can store their initial token supply on Cardano as Cardano native tokens.
Creation of the native token is not supported by this wizard and must be performed manually before this step.
> Do you want to configure a native token for you Partner Chain? No
Chain configuration (pc-chain-config.json) is now ready for distribution to network participants.

If you intend to run a chain with permissioned candidates, you must manually set their keys in the pc-chain-config.json file before proceeding. Here's an example of how to add permissioned candidates:

{
  ...
  "initial_permissioned_candidates": [
    {
      "aura_pub_key": "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde49a5684e7a56da27d",
      "grandpa_pub_key": "0x88dc3417d5058ec4b4503e0c12ea1a0a89be200f498922423d4334014fa6b0ee",
      "sidechain_pub_key": "0x020a1091341fe5664bfa1782d5e0477968906ac916b04cb365ec3153755684d9a1"
    },
    {
      "aura_pub_key": "0x8eaf04151687736326c9fea17e25fc5287613698c912909cb226aa4794f26a48",
      "grandpa_pub_key": "0xd17c2d7823ebf260fd138f2d7e27d114cb145d968b5ff5006125f2414fadae69",
      "sidechain_pub_key": "0x0390084fdbf27d2b79d26a4f13f0cdd982cb755a661969143c37cbc49ef5b91f27"
    }
  ]
}

After setting up the permissioned candidates, execute the 'create-chain-spec' command to generate the final chain specification.
🚀 All done!
```

Perfect, now we need to update `pc-chain-config.json` with initial candidates. As decided, I'm gonna use the same keys as in devnet node 1-6.
To do so, simply copy initial_permissioned_candidates array from `res/devnet/pc-chain-config.json` into the generated `pc-chain-config.json`

Now, don't forget to copy the new `pc-chain-config.json` into `res/devnet/`.

## Chain spec creation

BUG: https://shielded.atlassian.net/browse/PM-18688

Working now on `governed-map-asset-policy-id-rename` branch.

```
./target/release/midnight-node wizards create-chain-spec
This wizard will create a chain spec JSON file according to the provided configuration, using WASM runtime code from the compiled node binary.
Chain parameters:
- Genesis UTXO: d8eca39a90a74d1da650581ed6269aaecc2cde769e6023fd5a18706b18bca4d7#1
SessionValidatorManagement Main Chain Configuration:
- committee_candidate_address: addr_test1wp6srw86tvcms65y6geg4qmgnwvpkd20evs0zm0pfdtv8hqadv3l9
- d_parameter_policy_id: ba0d04dd197bf3a663c487cd0360c0c6a496dcf8bd52e73655319695
- permissioned_candidates_policy_id: f1206a4ddef8807d8aac05faf058bdfe960891be8f80f02027559aaf
Native Token Management Configuration (unused if empty):
- asset name: 0x
- asset policy ID: 0x00000000000000000000000000000000000000000000000000000000
- illiquid supply address: addr_test1wrr8e7s5pqv0zc3zwhd6v4tnfvkmwscvqgpnxauqfd5yj8qktalge
Governed Map Configuration:
- validator address: addr_test1wpdrv3yq2me42ud8htn23mp824pnxgk7cq2a9l5z3tj4ldq96v8y5
- asset policy ID: ec4ff2e365a5cc1552cdbb897df622170f251eeeaeb7360745f31f31
Initial permissioned candidates:
- Partner Chains Key: 0x03b2c9e72815b783358ace09a9a4b1d21366604691b22f18bb231deaee6116bb0b, AURA: 0xc28906486ffd3ab7e46df7f5d707a63aa8355711a8a1b1e124816a0d15d3d037, GRANDPA: 0xb009d0502e7d68db5a0e76eb148d4789fe10c9ae8731dfc426397da559b62504
- Partner Chains Key: 0x03d23dd221b9102103b573a3e4e725005195848de47360d9a5eae77a9e0ff69f65, AURA: 0xc4fc2a962ddff5fc2d1bd0f17adcfb44831d1523421a22b239b02a2e9ac11c26, GRANDPA: 0xbc78c5946272bb731480710eba9e77c91d001d9b480c880fe8289d07ad236ba1
- Partner Chains Key: 0x039da102a5a73d65985a24e9fd5dcb0820ec6d6273e24e7a2f9bd320485f4e25ce, AURA: 0x7228f0de854a09ca3cdb0f2591eb69f126c4a80c092fba7f0bbc1a6e5aa09e7d, GRANDPA: 0xbc8b55f8d8e26a071c79f170a10b9c1176f7f4344d83e954b0d2972de0cd1bb2
- Partner Chains Key: 0x03549f228e846792138220a7d08c7df93323945716b916c6858a5c882420ec8680, AURA: 0xec26fbe2852f44bc9e3dca604f078128ce14a89ef38eb584fa79f10dd2db3f75, GRANDPA: 0xfd9853bb0aeedd3a2989875d3ebba7520b82f456df8397d9a3b2b48b876017ca
- Partner Chains Key: 0x022aa7e7fae45dc8c8ab67b914d60d0ccfade2182159e4f3f472e9c83d039cc706, AURA: 0xd8308970c143b5a8426842434c1ab4d1fbca601c667e2b6b232533cf824aa27b, GRANDPA: 0x058ff99d430c4f572ce3742749632e0402a796d5fdca52afe738e2e1b3c7125a
- Partner Chains Key: 0x02c5b746231fb0d4003d45b057a843377a3a1024bc1f23f48926877826f83e11db, AURA: 0x749f270a740e8ce192343cd2691c5cda3f06ba0aceb965ade62f3805c9d9b95b, GRANDPA: 0x0f664d0ee4a9d6b50acd95ff520f94018c46c09cc39a9123bc695a0c655018cf
> Do you want to continue? Yes
running external command: /Users/radek/work/repos/midnight-node.worktrees/main/target/release/midnight-node build-spec --disable-default-bootnode > chain-spec.json
command output: 2025-08-07 16:06:33 Building chain spec    
chain-spec.json file has been created.
If you are the governance authority, you can distribute it to the validators.
Run 'setup-main-chain-state' command to set D-parameter and permissioned candidates on Cardano.
```

## Setup main chain state

```
./target/release/midnight-node wizards setup-main-chain-state 
This wizard will set or update D-Parameter and Permissioned Candidates on the main chain. Setting either of these costs ADA!
Will read the current D-Parameter and Permissioned Candidates from the main chain using Ogmios client.
> Ogmios protocol (http/https) https
> Ogmios hostname ogmios.devnet.midnight.network
> Ogmios port 443
List of permissioned candidates is not set on Cardano yet.
> Do you want to set/update the permissioned candidates on the main chain with values from configuration file? Yes
> path to the payment signing key file res/devnet/governance.skey
2025-08-07T16:12:24.959204+02:00 INFO partner_chains_cardano_offchain::permissioned_candidates - There aren't any permissioned candidates. Preparing transaction to insert.
2025-08-07T16:12:25.255434+02:00 INFO partner_chains_cardano_offchain::multisig - 'Insert Permissioned Candidates' transaction submitted: a8d79073b27a4778af601298c7278f291dfc257c1ceef979249f952be64baeb0
2025-08-07T16:12:25.255590+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output 'a8d79073b27a4778af601298c7278f291dfc257c1ceef979249f952be64baeb0#0'
2025-08-07T16:12:30.316761+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output 'a8d79073b27a4778af601298c7278f291dfc257c1ceef979249f952be64baeb0#0'
2025-08-07T16:12:35.378798+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output 'a8d79073b27a4778af601298c7278f291dfc257c1ceef979249f952be64baeb0#0'
2025-08-07T16:12:40.438375+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output 'a8d79073b27a4778af601298c7278f291dfc257c1ceef979249f952be64baeb0#0'
2025-08-07T16:12:40.497145+02:00 INFO partner_chains_cardano_offchain::await_tx - Transaction output 'a8d79073b27a4778af601298c7278f291dfc257c1ceef979249f952be64baeb0'
Permissioned candidates updated. The change will be effective in two main chain epochs.
> Do you want to set/update the D-parameter on the main chain? Yes
> Enter P, the number of permissioned candidates seats, as a non-negative integer. 1100
> Enter R, the number of registered candidates seats, as a non-negative integer. 100
> path to the payment signing key file res/devnet/governance.skey
2025-08-07T16:13:32.985009+02:00 INFO partner_chains_cardano_offchain::d_param - There is no D-parameter set. Inserting new one.
2025-08-07T16:13:34.255140+02:00 INFO partner_chains_cardano_offchain::multisig - 'Insert D-parameter' transaction submitted: 3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98
2025-08-07T16:13:34.255256+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:13:39.311647+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:13:44.366236+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:13:49.424214+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:13:54.483929+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:13:59.542712+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:14:04.601578+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:14:09.656421+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:14:14.715009+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:14:19.771011+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:14:24.829074+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:14:24.883617+02:00 INFO partner_chains_cardano_offchain::await_tx - Transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98'
2025-08-07T16:14:24.883643+02:00 INFO partner_chains_cardano_offchain::await_tx - Probing for transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98#0'
2025-08-07T16:14:24.947578+02:00 INFO partner_chains_cardano_offchain::await_tx - Transaction output '3537051bfe2adb3f6e3316a0684701fdbe9fe49cb5a70f34f9bc531037587b98'
D-parameter updated to (1100, 100). The change will be effective in two main chain epochs.
Done. Please remember that any changes to the Cardano state can be observed immediately, but from the Partner Chain point of view they will be effective in two main chain epochs.
```

## Verify DParam

Start your node:
```
export DB_SYNC_POSTGRES_CONNECTION_STRING="postgres://cardano@localhost:54322/cexplorer"
export CARDANO_SECURITY_PARAMETER=432                          # <----- not sure why?
./target/release/midnight-node
```

Get Ariadne parameters:
```
curl --request POST \
  --url http://localhost:9944/ \
  --header 'Content-Type: application/json' \
  --data '{
	"jsonrpc": "2.0",
  "method": "sidechain_getAriadneParameters", 
  "params": [1019],
  "id": 1
}'
```

SUCCESS!
```
{
	"jsonrpc": "2.0",
	"id": 1,
	"result": {
		"dParameter": {
			"numPermissionedCandidates": 1100,
			"numRegisteredCandidates": 100
		},
		"permissionedCandidates": [
			{
				"sidechainPublicKey": "0x022aa7e7fae45dc8c8ab67b914d60d0ccfade2182159e4f3f472e9c83d039cc706",
				"auraPublicKey": "0xd8308970c143b5a8426842434c1ab4d1fbca601c667e2b6b232533cf824aa27b",
				"grandpaPublicKey": "0x058ff99d430c4f572ce3742749632e0402a796d5fdca52afe738e2e1b3c7125a",
				"isValid": true
			},
			{
				"sidechainPublicKey": "0x02c5b746231fb0d4003d45b057a843377a3a1024bc1f23f48926877826f83e11db",
				"auraPublicKey": "0x749f270a740e8ce192343cd2691c5cda3f06ba0aceb965ade62f3805c9d9b95b",
				"grandpaPublicKey": "0x0f664d0ee4a9d6b50acd95ff520f94018c46c09cc39a9123bc695a0c655018cf",
				"isValid": true
			},
			{
				"sidechainPublicKey": "0x03549f228e846792138220a7d08c7df93323945716b916c6858a5c882420ec8680",
				"auraPublicKey": "0xec26fbe2852f44bc9e3dca604f078128ce14a89ef38eb584fa79f10dd2db3f75",
				"grandpaPublicKey": "0xfd9853bb0aeedd3a2989875d3ebba7520b82f456df8397d9a3b2b48b876017ca",
				"isValid": true
			},
			{
				"sidechainPublicKey": "0x039da102a5a73d65985a24e9fd5dcb0820ec6d6273e24e7a2f9bd320485f4e25ce",
				"auraPublicKey": "0x7228f0de854a09ca3cdb0f2591eb69f126c4a80c092fba7f0bbc1a6e5aa09e7d",
				"grandpaPublicKey": "0xbc8b55f8d8e26a071c79f170a10b9c1176f7f4344d83e954b0d2972de0cd1bb2",
				"isValid": true
			},
			{
				"sidechainPublicKey": "0x03b2c9e72815b783358ace09a9a4b1d21366604691b22f18bb231deaee6116bb0b",
				"auraPublicKey": "0xc28906486ffd3ab7e46df7f5d707a63aa8355711a8a1b1e124816a0d15d3d037",
				"grandpaPublicKey": "0xb009d0502e7d68db5a0e76eb148d4789fe10c9ae8731dfc426397da559b62504",
				"isValid": true
			},
			{
				"sidechainPublicKey": "0x03d23dd221b9102103b573a3e4e725005195848de47360d9a5eae77a9e0ff69f65",
				"auraPublicKey": "0xc4fc2a962ddff5fc2d1bd0f17adcfb44831d1523421a22b239b02a2e9ac11c26",
				"grandpaPublicKey": "0xbc78c5946272bb731480710eba9e77c91d001d9b480c880fe8289d07ad236ba1",
				"isValid": true
			}
		],
		"candidateRegistrations": {}
	}
}
```
