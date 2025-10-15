# Toolkit (JS)

## Usage

The toolkit provides commands for executing `compactc` compiled contracts. It requires a configuration file, written
in TypeScript, that provides a _binding_ between the compiled contract (i.e., the generated `compactc` output),
and its assets. Often, compiled contracts require the implementation of witnesses that perform some utility for
the contract in regard to the caller's private state. The configuration file can provide these witness implementations.

An example of the `'contract.config.ts'` file is shown below:

```ts
/**
 * This module provides configuration to the `node-toolkit(js)` command line. Like most JS/TS based configuration
 * files, it is an executable module that has a _default_ export, which for our purposes, defines:
 *
 * 1. A built _contract executable_. This is a binding between a compiled contract (i.e., the generated `compactc`
 * output) and its other logical assets,
 * 2. A function that provides some initial private state,
 * 3. A collection of configuration values (that can also be overridden by environment variables, or on the command
 * line).)
 *
 * @module
 */

import { CompiledContract, ContractExecutable, type Contract } from '@midnight-ntwrk/compact-js/effect';
import { Contract as C_ } from './managed/counter/contract/index.cjs';

/**
 * A type that describes the private state of the contract.
 */
type PrivateState = {
  count: number;
};

// A type alias to the imported Contract type (that binds it to our type of private state).
type CounterContract = C_<PrivateState>;
// Rename the imported Contract constructor so that we have a more distinct name. Unfortunately, `compactc`
// always generates using the name `Contract`.
const CounterContract = C_;

/**
 * An object that represents the witness functions defined by the compiled contract.
 */
const witnesses: Contract.Contract.Witnesses<CounterContract> = {
  // In this example, we simply increment the count stored in our private state.
  private_increment: ({ privateState }) => [{ count: privateState.count + 1 }, []]
}

/**
 * Creates the initial private state to use when deploying new instances of the contract.
 *
 * @returns An initialized object representing {@link PrivateState}.
 */
const createInitialPrivateState: () => PrivateState = () => ({ count: 0 });

export default {
  // Use the imports from `@midnight-ntwrk/compact-js/effect` to build an executable contract (an object)
  // that binds the output from `compactc` to the physical and logical assets that are required for its
  // execution.
  contractExecutable: CompiledContract.make<CounterContract>('CounterContract', CounterContract).pipe(
    // If the contract has no witnesses, then the `witnesses` const can be removed, and use
    // CompiledContract.withVacantWitnesses instead:
    CompiledContract.withWitnesses(witnesses),
    CompiledContract.withCompiledFileAssets('./managed/counter'),
    ContractExecutable.make
  ),
  createInitialPrivateState,
  // Configuration can also be provided here. 
  config: {
    keys: {
      coinPublic: 'd2dc8d175c0ef7d1f7e5b7f32bd9da5fcd4c60fa1b651f1d312986269c2d3c79',
    },
    network: 'undeployed'
  }
}
```

> [!TIP]
> In the examples that follow, when running the command from its own folder, replace `midnight-node-toolkit-js`
with `./dist/bin.js` or `npm start`. The binary name will only be registered when the package is installed globally.
>
> Note: If you run via `npm start`, then you'll need to separate the toolkit arguments from `npm`s by adding a `--`.  
> E.g., `npm start -- deploy ...`

#### Global Options
The following global options can be used across all commands:

- `-c | --config <file>`  
**Optional** A path to the contract configuration file.  
Defaults to using `'contract.config.ts'` in the working directory.

- `-o | --output <file>`  
**Optional** A path to the where the produced 'Intent' data should be serialized.  
Defaults to writing to `'output.bin'` in the working directory.

The following global options can be used across all commands, and may be provided as values in the contract
configuration file, through environment variables, or via its command line option:

- `-p | --coin-public <key>` (`KEYS_COIN_PUBLIC=<key>`)  
A user public key capable of receiving Zswap coins, in hex or Bech32m format.
```ts
config: {
  keys: {
    ...
    coinPublic: '<key>'
    ...
  }
}
```

#### Deploying
```bash
midnight-node-toolkit-js deploy [...global_options] [-s | --signing <key>] <arg>...
```

#### Options
The `deploy` command accepts the following options, and may be provided as values in the contract configuration
file, through environment variables, or via its command line option:

- `-s | --signing <key>` (`KEYS_SIGNING=<key>`)  
**Optional** A BIP-340 signing key, in hex format.  
A signing key is used to create a Contract Maintenance Authority (CMA) when initializing the new contract. It is
used to create a verifying key that is included in the contract deployment data that will be included in the
serialized Intent.
```ts
config: {
  keys: {
    ...
    signing: '<key>'
    ...
  }
}
```

#### Arguments
Arguments are forwarded to the contract constructor in the order in which they are received on the command line.

#### Circuit Invocation
```bash
midnight-node-toolkit-js circuit [...global_options] --input <file> <address> <circuit_id> <arg>...
```

#### Options
The `circuit` command accepts the following options via the command line:

- `-i | --input <file>`  
A path to a file containing the serialized onchain (or Ledger) state that represents the _current_ state of
the contract. The executing circuit will apply to this given state.

#### Arguments
The `circuit` command requires the following arguments:

- `address`  
The contract address.

- `circuit_id`  
The name of the circuit that is to be invoked.

Any remaining arguments are forwarded to the contract circuit in the order in which they are received on the
command line.

#### Contract Maintenance
```bash
midnight-node-toolkit-js maintain contract [...global_options] --input <file> <address> <circuit_id> <arg>...
```

#### Options
The `maintain contract` command accepts the following options via the command line:

- `-i | --input <file>`  
A path to a file containing the serialized onchain (or Ledger) state that represents the _current_ state of
the contract.

- `-s | --signing <key>` (`KEYS_SIGNING=<key>`)  
**Optional** A BIP-340 signing key, in hex format.  
The signing key to use when signing the maintenance update data.

#### Arguments
The `maintain contract` command requires the following arguments:

- `address`  
The contract address.

- `new_signing_key`  
The new signing key to use in future maintenance operations. Note: This should not be the same as the key
specified with the `-s | --signing` option.

#### Circuit Maintenance
```bash
midnight-node-toolkit-js maintain circuit [...global_options] --input <file> <address> <circuit_id> <arg>...
```

#### Options
The `maintain contract` command accepts the following options via the command line:

- `-i | --input <file>`  
A path to a file containing the serialized onchain (or Ledger) state that represents the _current_ state of
the contract.

- `-s | --signing <key>` (`KEYS_SIGNING=<key>`)  
**Optional** A BIP-340 signing key, in hex format.  
The signing key to use when signing the maintenance update data.

#### Arguments
The `maintain contract` command requires the following arguments:

- `address`  
The contract address.

- `circuit_id`  
The name of the circuit to maintain.

- `verifier_key_path`  
**Optional** A path to the verifier key to insert or update for the circuit identified by `circuit_id`. If not
present, the `circuit_id` is removed from the contract state.
