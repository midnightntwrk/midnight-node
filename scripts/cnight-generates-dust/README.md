Test scripts
============

# **NOTE:** If testing cnight-generates-dust functionality, it's recommended to use the node project in the `/tests` directory rather than these scripts.

All test scripts assume addresses on Preprod Testnet.  To use Mainnet edit all
scripts by replacing `--testnet-magic 2` flag to cardano-cli with `--mainnet`.

A brief summary of what the scripts do:

  * `mkWallets.sh` - create new test wallets for Alice and Bob.  Generate
    address files needed later and provide Base16 encodings of wallet addresses
    that need to be put into `datum-*.json` files.

  * `mkCollateral.sh` - create a collateral UTxO in the wallet and save it to a
    file.

  * `mkHashes.sh` - display currency symbols and addresses to observe.
    Cache these hashes in files for later use with transactions.

  * `register.sh` - submit registration to the mapping validator.  Mints an
    authentication token and attaches user's registration datum.

  * `deregister.sh` - remove existing registration.  Burns authentication token.
    Requires editing before each call, i.e. registration UTxO must be pointed to
    explicitly.

  * `receive_cnight.sh` - mint 10 cNIGHT tokens.  These are dummy tokens made
    only for testing.

Test procedure:

  1. Create wallet using `mkWallets.sh`.

  2. Modify `datum-alice.json` to include Base16-encoded wallet address returned
     by `mkWallets.sh`.  Put it into the first `bytes` field.  Do the same for
     Bob.  NOTE: DUST address field in both datums is garbage at the moment.

  3. Add funds to wallets, e.f. from [faucet](https://docs.cardano.org/cardano-testnets/tools/faucet)

  4. Run `mkCollateral.sh alice` and `mkCollateral.sh bob`.

  5. Run `mkHashes.sh`.  This returns address to observe, authentication token
     policy ID and (dummy) cNIGHT policy ID.

  6. Run `register.sh alice` and `register.sh bob` to register these two users
     for DUST production.  Can be repeated.

  7. Run `receive_cnight.sh alice` and `receive_cnight.sh bob` to add cNIGHT
     tokens to wallets.  These tokens can then be transferred between wallets to
     generate observable events.

  8. Run `deregister.sh alice` and `deregister.sh bob` to deregister these two
     users for DUST production.  Can be repeated as long as they are registered.
     This script requires supplying registration UTxO manually, meaning it must
     be edited before every call.

