import { CompiledContract, ContractExecutable, type Contract } from '@midnight-ntwrk/compact-js/effect';
import { Contract as BBoardContract_ } from './out/contract/index.js';

type BBoardPrivateState = {
  readonly secretKey: string;
};

type BBoardContract = BBoardContract_<BBoardPrivateState>;
const BBoardContract = BBoardContract_;

const witnesses: Contract.Contract.Witnesses<BBoardContract> = {
  localSecretKey: ({ privateState }) => [
    privateState,
    new Uint8Array(Buffer.from(privateState.secretKey, 'hex')),
  ],
};

const createInitialPrivateState: () => BBoardPrivateState = () => ({
  secretKey: '{{SECRET_KEY}}',
});

export default {
  contractExecutable: CompiledContract.make<BBoardContract>(
    'BBoardContract',
    BBoardContract,
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
