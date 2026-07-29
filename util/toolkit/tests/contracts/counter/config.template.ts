import { CompiledContract, ContractExecutable, type Contract } from '@midnight-ntwrk/compact-js/effect';
import { Contract as CounterContract_ } from './out/contract/index.js';

type CounterPrivateState = {
  readonly privateCounter: number;
};

type CounterContract = CounterContract_<CounterPrivateState>;
const CounterContract = CounterContract_;

const witnesses: Contract.Contract.Witnesses<CounterContract> = {
  privateIncrement: ({ privateState }) => [
    { privateCounter: privateState.privateCounter + 1 },
    [],
  ],
};

const createInitialPrivateState: () => CounterPrivateState = () => ({
  privateCounter: 0,
});

export default {
  contractExecutable: CompiledContract.make<CounterContract>(
    'CounterContract',
    CounterContract,
  ).pipe(
    CompiledContract.withWitnesses(witnesses),
    CompiledContract.withCompiledFileAssets('./out'),
    ContractExecutable.make,
  ),
  createInitialPrivateState,
  config: {
    keys: {
      coinPublic: '{{COIN_PUBLIC}}',
    },
    network: '{{NETWORK}}',
  },
};
