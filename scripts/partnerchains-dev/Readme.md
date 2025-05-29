# How to - with Partner Chains

These scripts are intended to be used with the `./partnerchains-dev` script in the root of the node repo. This will port-forward from k8s and setup all the tools you need.

These notes were created by following the [official partner-chains guide](https://github.com/input-output-hk/partner-chains/blob/master/docs/user-guides/chain-builder.md).

These scripts work well in conjunctions with the `earthly +generate-<network>-keys` targets.

# If you're updating an existing network, use:

**Make sure you answer "No" to the question "Do you want to configure a native token for you Partner Chain?" (last prompt)**
```
./update-network.sh <network-name>
```
After running this script, jump straight to the "Create the chain" section.

The script accepts two flags:
- `--local`: To help with developemnt. It loads the `partnerchains-dev` from your local instead of using the ones from the image.
- `--push-image`: Also to help with development. It pushes a new `partnerchains-dev` build from your local. It saves you from having to push changes to the repo and run the Github action.

## Run script to generate the first node keys

Run the following command to generate a node key. This is required for the next steps. For any prompts, use the default values (press Return until the command completes):
```
$ ./generate-key.sh
```

## Create a new wallet

You can now create a new wallet using the following command:
```
$ ./generate-governance-wallet.sh
```

## Run the chain builder wizard

Run the following command to start the chain builder wizard with the following options:
```
$ ./partner-chains-cli prepare-configuration

> node base path ./data
> Your bootnode should be accessible via: hostname
> Enter bootnode TCP port 3033
> Enter bootnode hostname localhost
> Is the governance authority displayed above correct? Yes
> partner chain id 1111
> Which cardano network would you like to use? preview
> Ogmios protocol (http/https) https
> Ogmios hostname ogmios.preview.midnight.network
> Ogmios port 443
> Do you want to configure a native token for you Partner Chain? No
```

Then run:
```
$ ./partner-chains-cli create-chain-spec
```

## Create the chain

Use the [Cardano Testnet Faucet](https://docs.cardano.org/cardano-testnets/tools/faucet/) to get some Testnet ADA. This faucet gives you 10000 ADA, which is plenty.

It's easiest to use the governance wallet address you created earlier to pay for the transaction. To find the address of this wallet, run:

```
$ cat governance-wallet/payment.addr
```

Ensure you've filled out the `initial_permissioned_candidate` key of the `partner-chains-cli-chain-config.json` file in the following format:
```json
  "initial_permissioned_candidates": [
    {
      "sidechain_pub_key": "0x0356dcb690685b6b159606ab16fae7417691e499f6ca205238d48515e9eaa3a8a0",
      "aura_pub_key": "0xa237d084859b99078c28f850b3398594d8419c2866b234b0e3e6f293676af26a",
      "grandpa_pub_key": "0x4a3b21c7d37563f02ae231926dbe7201db63d00c9cbe2105f34ab0ce4f0c0c5a"
    },
```

Run the following command to create the chain:
```
$ ./partner-chains-cli setup-main-chain-state
```

## Updating the D parameter or permissioned candidates

Use the same steps as above (**Create the chain**) to update any on-chain parameters.

## Move the files to the correct network in the `res` folder

Save the `payment.skey` file to AWS secrets and check in all public data to git.

# Migrations
## v1.4.0
```
./1.4.0-migration.sh <network-name>
```

Registers the chain in the new smart contract version and save the new `partner-chains-cli-chain-config.json` and `chain-spec.json` files locally under `/res/<network-name>/1.4.0-migration`
