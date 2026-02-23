import type * as __compactRuntime from '@midnight-ntwrk/compact-runtime';

export type Witnesses<PS> = {
  find(context: __compactRuntime.WitnessContext<Ledger, PS>, content_0: bigint): [PS, { leaf: bigint,
                                                                                        path: { sibling: { field: bigint
                                                                                                         },
                                                                                                goes_left: boolean
                                                                                              }[]
                                                                                      }];
}

export type ImpureCircuits<PS> = {
  store(context: __compactRuntime.CircuitContext<PS>, something_0: bigint): __compactRuntime.CircuitResults<PS, []>;
  check(context: __compactRuntime.CircuitContext<PS>, something_0: bigint): __compactRuntime.CircuitResults<PS, []>;
}

export type PureCircuits = {
}

export type Circuits<PS> = {
  store(context: __compactRuntime.CircuitContext<PS>, something_0: bigint): __compactRuntime.CircuitResults<PS, []>;
  check(context: __compactRuntime.CircuitContext<PS>, something_0: bigint): __compactRuntime.CircuitResults<PS, []>;
}

export type Ledger = {
}

export type ContractReferenceLocations = any;

export declare const contractReferenceLocations : ContractReferenceLocations;

export declare class Contract<PS = any, W extends Witnesses<PS> = Witnesses<PS>> {
  witnesses: W;
  circuits: Circuits<PS>;
  impureCircuits: ImpureCircuits<PS>;
  constructor(witnesses: W);
  initialState(context: __compactRuntime.ConstructorContext<PS>): __compactRuntime.ConstructorResult<PS>;
}

export declare function ledger(state: __compactRuntime.StateValue | __compactRuntime.ChargedState): Ledger;
export declare const pureCircuits: PureCircuits;
