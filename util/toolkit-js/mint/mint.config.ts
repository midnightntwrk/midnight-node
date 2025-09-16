import { CompiledContract, ContractExecutable, type Contract } from '@midnight-ntwrk/compact-js/effect';
import { Contract as C_ } from './out/contract/index.cjs';

/**
 * A type that describes the private state of the contract.
 */
type PrivateState = {};

// A type alias to the imported Contract type (that binds it to our type of private state).
type MintContract = C_<PrivateState>;
const MintContract = C_;

/**
 * An object that represents the witness functions defined by the compiled contract.
 */
const witnesses: Contract.Contract.Witnesses<MintContract> = {
  // In this example, we simply increment the count stored in our private state.
  something: () => [{}, []]
}

const createInitialPrivateState: () => PrivateState = () => ({});

export default {
  // Use the imports from `@midnight-ntwrk/compact-js/effect` to build an executable contract (an object)
  // that binds the output from `compactc` to the physical and logical assets that are required for its
  // execution.
  contractExecutable: CompiledContract.make<MintContract>('MintContract', MintContract).pipe(
    CompiledContract.withWitnesses(witnesses),
    CompiledContract.withZKConfigFileAssets('./out'),
    ContractExecutable.make
  ),
  createInitialPrivateState,
  // Configuration can also be provided here. 
  config: {
    keys: {
      // Seed: 0000000000000000000000000000000000000000000000000000000000000001
      coinPublic: 'aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98',
    },
    network: 'undeployed'
  }
}
