/**
 * This module provides configuration to the `node-toolkit(js)` command line. Like most JS/TS based configuration
 * files, it is an executable module that has a _default_ export, which for our purposes, defines:
 *
 * 1. A built _contract executable_. This is a binding between a compiled contract (i.e., the generated `compactc`
 * output) and its other logical assets,
 * 2. A function that provides some initial private state,
 * 3. A collection of configuration values (that can also be overridden by environment variables, or on the command
 * line).
 *
 * @module
 */

import { CompiledContract, ContractExecutable, type Contract } from '@midnight-ntwrk/compact-js/effect';
import { Contract as C_ } from './managed/counter/contract/index.js';

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
  private_increment: ({ privateState }, amount) => [{ count: privateState.count + Number(amount) }, []],
  private_decrement: ({ privateState }, amount) => [
    { count: privateState.count - Number(amount as unknown as bigint) },
    []
  ],
  private_reset: () => [{ count: 0 }, []]
};

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
