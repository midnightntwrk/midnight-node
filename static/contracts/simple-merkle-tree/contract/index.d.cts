import type * as __compactRuntime from '@midnight-ntwrk/compact-runtime';

export type Witnesses<T> = {
  find(context: __compactRuntime.WitnessContext<Ledger, T>, content_0: bigint): [T, { leaf: bigint,
                                                                                      path: { sibling: { field: bigint
                                                                                                       },
                                                                                              goes_left: boolean
                                                                                            }[]
                                                                                    }];
}

export type ImpureCircuits<T> = {
  store(context: __compactRuntime.CircuitContext<T>, something_0: bigint): __compactRuntime.CircuitResults<T, []>;
  check(context: __compactRuntime.CircuitContext<T>, something_0: bigint): __compactRuntime.CircuitResults<T, []>;
}

export type PureCircuits = {
}

export type Circuits<T> = {
  store(context: __compactRuntime.CircuitContext<T>, something_0: bigint): __compactRuntime.CircuitResults<T, []>;
  check(context: __compactRuntime.CircuitContext<T>, something_0: bigint): __compactRuntime.CircuitResults<T, []>;
}

export type Ledger = {
}

export type ContractReferenceLocations = any;

export declare const contractReferenceLocations : ContractReferenceLocations;

export declare class Contract<T, W extends Witnesses<T> = Witnesses<T>> {
  witnesses: W;
  circuits: Circuits<T>;
  impureCircuits: ImpureCircuits<T>;
  constructor(witnesses: W);
  initialState(context: __compactRuntime.ConstructorContext<T>): __compactRuntime.ConstructorResult<T>;
}

export declare function ledger(state: __compactRuntime.StateValue): Ledger;
export declare const pureCircuits: PureCircuits;
