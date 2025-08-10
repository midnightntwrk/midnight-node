# Proposal 0001: Configuration Overhaul

This proposal is created to address the current configuration complexity and
fragility in deployments of the node application.

## Problem Statement

There are many different sources of configuration, and places where
configuration can be overridden. This includes:

- Command line arguments

    These can be found in `node/src/command.rs` as separate commands. Each
    command uses `clap` to handle arguments. Most of these can remain unchanged
    and are useful utilities, for example the `key` subcommand is useful for
    generating keys for new deployments. In the case these commands rely on
    environment variables too, this should be examined.

- Environment Variables

    Environment variables are used to configure the node application in many
    cases. The keys for these environment variables are spread out across the
    codebase, in some cases inside dependent libraries.

- Configuration files from Partner Chains

    There are two configuration files that we are given from Partner chains:
    `addresses.json` and `mock_*.json`. The addresses file lists values that
    the partner chains pallets need to communicate with the main Cardano
    chain. The `mock` json files are used to mock the bridge with the Cardano
    chain for testing purposes.

- `boot.sh`

    There is a shell file that acts as the main runner inside the Docker image.
    It groups together commandline arguments and environment variables into
    common use cases. For example, launching a node in "dev" mode (single node
    network), in "validator" mode as a peer, as an archive node, etc. This file
    produces fragility in our releases when it is not updated in-line with the
    most recent CLI and Environment variable updates.

- Git-ops

    Helm charts exist in our git-ops repository to handle configuration of node
    deployments. If these are out-of-date with our node, the deployment fails.

- Helm chart templates

    We have templates for our helm charts in a separate repository. The values
    exposed in these templates do not always match the names of the environment 
    variables they're setting.

The result of this is uncertainty on how and where values are set, and a need
to be incredibly vigilant when updating a configuration value. You need to
understand the whole chain of how and where environment variables are used to
ensure you don't make a mistake that could cause a failed deployment.

There is also no easy way to view the current configuration of a node.

## Proposed Changes

The proposed change takes [12factor](https://12factor.net/config) as a base, but with some modification. 

With this in mind, we propose the following:

### When the node binary is running as a service, everything is configured through environment variables

Without any command specified as an argument, the node will run in the most
common customer-facing mode. In other cases, for example local dev mode, there
will be a command for that e.g. `./node local-dev`. In this case, that will be
the only command line argument.

This means <=1 command line arguments when the node is running as a service.

The work involved to achieve this is non-trivial - we will need to match the
command-line arguments that are included in the default substrate node `run`
command.
[In practice, this means instantiating this struct using environment variables.](https://github.com/paritytech/polkadot-sdk/blob/18ed309a37036db8429665f1e91fb24ab312e646/substrate/client/cli/src/commands/run_cmd.rs#L46) 
This could be achieved by duplicating or forking the struct to add `env`
parameters on everything, then populating the environment with our config
before parsing it. However, if we did this we would have to license this code
under GPL.

The alternative is to initialising the struct using our own environment
variable config struct. This involves no duplication of code, but we would have
to ensure it remains up-to-date with the substrate `run` struct. This will be
checked by the compiler, and any defaults can be read using the `clap` API
[(source)](https://docs.rs/clap_builder/4.5.8/src/clap_builder/builder/arg.rs.html#4050).

It should be noted that duplicating the entire `run` command functionality is
out-of-scope for this proposal.


### Sensible defaults for common use-cases

We will create a set of common defaults for each use-case, for example
`local-dev`, `seed`, and `validator` nodes. These will be baked-into the binary
as data. Environment variables will overwrite any values in these defaults, and
there will be an option to run without any defaults to align with twelve factor
methodology for configuration.

### A single file that describes all the configuration used by the node application

This means a developer can look at a single file/doc page and understand
exactly which set of environment variables they need.

### Where dependent libraries are using environment variables, we shadow them

These aliases are created at the application level to maintain a single source
of truth. The prime examples of this is `MIDNIGHT_LEDGER_STATIC_DIR` and
`ZKIR_PP` - at the application level, we would add these variables to our
configuration set. If we created aliases e.g. `MY_MIDNIGHT_LEDGER_STATIC_DIR`
that sets `MIDNIGHT_LEDGER_STATIC_DIR` internally, we may run into
inconsistencies.

### Changes to configuration defaults will be checked in CI

This will ensure that invalid configuration does not make it's way into
releases. We will verify that any keys in the default environment configs are
valid configuration keys.

### Retain the useful subcommands in the node binary

There are many useful subcommands that are used in the node binary. We should
retain these commands, and if they use environment variables make them visible
at the top-level in the same way as when the node is running as a service.
Command line arguments are allowed for these subcommands.


### Remove `boot.sh`

With the above changes implemented, there will be no need for `boot.sh` - common 
use-cases will be added as run-modes of the node.

**Implementation details**

We will define default configs in the `res` crate at the root of the
repository as `toml` files. These will be deserialized at compile-time into
configuration structs included in the binary.

**Useful crates:**

- [config-rs](https://github.com/mehcode/config-rs/tree/master) - good support for 12-factor app configuration, and allows easy 
overriding of environment variables with default sets. [Example.](https://github.com/mehcode/config-rs/tree/master)
- [serde-ignored](https://github.com/dtolnay/serde-ignored) - we can use this to check that our default configurations 
match our configuration structs strictly.

## Desired Result
The acceptance criteria for this work will be:

- Every configuration variable is documented
- Considerable reduction in the number of environment variables for SPOs needed to start the node[^1]
- Decrease the number of failures due to configuration errors

[^1] The docker compose example for SPOs shows the current complexity (taken from "Guide to becoming a block producer on the Midnight devnet"):

```
services:
 node:
   image: ghcr.io/midnight-ntwrk/midnight-node:0.3.2-4fd4fe8-ariadne
   ports:
     - "9944:9944"
   environment:
     CHAIN_ID: "1"
     THRESHOLD_NUMERATOR: "2"
     THRESHOLD_DENOMINATOR: "3"
     GENESIS_COMMITTEE_UTXO: "5f966ea504398e9d710336748a4368772c299fe7158af5cb34a9b384c53396b7#0"
     GOVERNANCE_AUTHORITY: "93f21ad1bba9ffc51f5c323e28a716c7f2a42c5b51517080b90028a6"
     COMMITTEE_HASH_VALIDATOR_HASH: "{Your committee hash validator hash}"
     COMMITTEE_CANDIDATE_ADDRESS: "{Your committee candidate address}"
     COMMITTEE_NFT_ADDRESS: "addr_test1wpsd3k6kw38d88hnfw7nc8efs7pyp6gxnenvm8xf0ag42asfv64dw"
     COMMITTEE_NFT_POLICY_ID: "{Your committee nft policy id}"
     DISTRIBUTED_SET_NFT_POLICY_ID: "{Your distributed set nft policy id}"
     DISTRIBUTED_SET_NFT_ADDRESS: "addr_test1wzchpz5ctg22quepnk9h532ygelasvss0z8ex539n5pj35qkqu4lf"
     MERKLE_ROOT_NFT_ADDRESS: "addr_test1wqlsxped56dwqpk96qfewlw7079v8v0hnh7chc6p6kgcxygy70qx5"
     D_PARAMETER_POLICY_ID: "c05402039e8f66cc16b938e670fd81920a717b10f8ff01ca45723d10"
     FUEL_MINTING_POLICY_ID: "{Your fuel minting policy id}"
     MERKLE_ROOT_NFT_POLICY_ID: "{Your merkle root nft policy id}"
     PERMISSIONED_CANDIDATES_POLICY_ID: "{Your permissioned candidates policy id}"
     POSTGRES_PASSWORD: "{YOUR POSTGRES DB PASSWORD}"
     POSTGRES_HOST: "{YOUR POSTGRES HOST}"
     POSTGRES_PORT: "5432"
     POSTGRES_USER: "{Your postgres db user}"
     POSTGRES_DB: "cexplorer"
     DB_SYNC_POSTGRES_CONNECTION_STRING: "psql://$(POSTGRES_USER):$(POSTGRES_PASSWORD)@$(POSTGRES_HOST):$(POSTGRES_PORT)/$(POSTGRES_DB)"
     CHAIN_SPEC: "/mn-src/publicDevnet.json"
     EXTRA_NODE_ARGS: "--bootnodes /dns/node-01.devnet.midnight.network/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp --validator"
     SEED_PHRASE: "{Your seed phrase}"
     USE_MAIN_CHAIN_FOLLOWER_MOCK: false

   healthcheck:
     test: [ "CMD", "curl", "-f", "http://localhost:9933/health" ]
     interval: 2s
     timeout: 5s
     retries: 5
     start_period: 40s
   volumes:
     - ./:/mn-src
```
