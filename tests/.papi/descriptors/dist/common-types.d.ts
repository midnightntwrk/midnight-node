// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { Enum, GetEnum, FixedSizeBinary, Binary, SS58String, ResultPayload, FixedSizeArray, TxCallData } from "polkadot-api";
type AnonymousEnum<T extends {}> = T & {
    __anonymous: true;
};
type MyTuple<T> = [T, ...T[]];
type SeparateUndefined<T> = undefined extends T ? undefined | Exclude<T, undefined> : T;
type Anonymize<T> = SeparateUndefined<T extends FixedSizeBinary<infer L> ? number extends L ? Binary : FixedSizeBinary<L> : T extends string | number | bigint | boolean | void | undefined | null | symbol | Uint8Array | Enum<any> ? T : T extends AnonymousEnum<infer V> ? Enum<V> : T extends MyTuple<any> ? {
    [K in keyof T]: T[K];
} : T extends [] ? [] : T extends FixedSizeArray<infer L, infer T> ? number extends L ? Array<T> : FixedSizeArray<L, T> : {
    [K in keyof T & string]: T[K];
}>;
export type I5sesotjlssv2d = {
    "nonce": number;
    "consumers": number;
    "providers": number;
    "sufficients": number;
    "data": Anonymize<I1q8tnt1cluu5j>;
};
export type I1q8tnt1cluu5j = {
    "free": bigint;
    "reserved": bigint;
    "frozen": bigint;
    "flags": bigint;
};
export type Iffmde3ekjedi9 = {
    "normal": Anonymize<I4q39t5hn830vp>;
    "operational": Anonymize<I4q39t5hn830vp>;
    "mandatory": Anonymize<I4q39t5hn830vp>;
};
export type I4q39t5hn830vp = {
    "ref_time": bigint;
    "proof_size": bigint;
};
export type I4mddgoa69c0a2 = Array<DigestItem>;
export type DigestItem = Enum<{
    "PreRuntime": Anonymize<I82jm9g7pufuel>;
    "Consensus": Anonymize<I82jm9g7pufuel>;
    "Seal": Anonymize<I82jm9g7pufuel>;
    "Other": Binary;
    "RuntimeEnvironmentUpdated": undefined;
}>;
export declare const DigestItem: GetEnum<DigestItem>;
export type I82jm9g7pufuel = [FixedSizeBinary<4>, Binary];
export type I57rgindhd6rdr = Array<{
    "phase": Phase;
    "event": Enum<{
        "System": Anonymize<I2crv8591gqcs5>;
        "Grandpa": GrandpaEvent;
        "Midnight": Anonymize<I19tmqo032ll7v>;
        "Balances": Anonymize<Iao8h4hv7atnq3>;
        "Sudo": Anonymize<I375j12p03cl6h>;
        "SessionCommitteeManagement": undefined;
        "RuntimeUpgrade": Anonymize<I3ol70vu5p7158>;
        "NativeTokenManagement": Anonymize<I3gp01gnl65els>;
        "NativeTokenObservation": Anonymize<Idrsj91vub2dkq>;
        "Preimage": PreimageEvent;
        "MultiBlockMigrations": Anonymize<I94co7vj7h6bo>;
        "PalletSession": Anonymize<I4co4bgsti676q>;
        "Scheduler": Anonymize<Ienmbdqscc5642>;
        "TxPause": Anonymize<I9ulgod11dfvq5>;
        "Session": SessionEvent;
    }>;
    "topics": Anonymize<Ic5m5lp1oioo8r>;
}>;
export type Phase = Enum<{
    "ApplyExtrinsic": number;
    "Finalization": undefined;
    "Initialization": undefined;
}>;
export declare const Phase: GetEnum<Phase>;
export type I2crv8591gqcs5 = AnonymousEnum<{
    /**
     * An extrinsic completed successfully.
     */
    "ExtrinsicSuccess": Anonymize<Ia82mnkmeo2rhc>;
    /**
     * An extrinsic failed.
     */
    "ExtrinsicFailed": Anonymize<I870h7pfmbjt5m>;
    /**
     * `:code` was updated.
     */
    "CodeUpdated": undefined;
    /**
     * A new account was created.
     */
    "NewAccount": Anonymize<Icbccs0ug47ilf>;
    /**
     * An account was reaped.
     */
    "KilledAccount": Anonymize<Icbccs0ug47ilf>;
    /**
     * On on-chain remark happened.
     */
    "Remarked": Anonymize<I855j4i3kr8ko1>;
    /**
     * An upgrade was authorized.
     */
    "UpgradeAuthorized": Anonymize<Ibgl04rn6nbfm6>;
    /**
     * An invalid authorized upgrade was rejected while trying to apply it.
     */
    "RejectedInvalidAuthorizedUpgrade": Anonymize<I7f4oosp2hcgjo>;
}>;
export type Ia82mnkmeo2rhc = {
    "dispatch_info": Anonymize<Ic9s8f85vjtncc>;
};
export type Ic9s8f85vjtncc = {
    "weight": Anonymize<I4q39t5hn830vp>;
    "class": DispatchClass;
    "pays_fee": Enum<{
        "Yes": undefined;
        "No": undefined;
    }>;
};
export type DispatchClass = Enum<{
    "Normal": undefined;
    "Operational": undefined;
    "Mandatory": undefined;
}>;
export declare const DispatchClass: GetEnum<DispatchClass>;
export type I870h7pfmbjt5m = {
    "dispatch_error": Anonymize<Im2qle9pka0f8>;
    "dispatch_info": Anonymize<Ic9s8f85vjtncc>;
};
export type Im2qle9pka0f8 = AnonymousEnum<{
    "Other": undefined;
    "CannotLookup": undefined;
    "BadOrigin": undefined;
    "Module": Enum<{
        "System": Anonymize<I5o0s7c8q1cc9b>;
        "Timestamp": undefined;
        "Aura": undefined;
        "Grandpa": Anonymize<I7q8i0pp1gkas6>;
        "Sidechain": undefined;
        "Midnight": Anonymize<I6nhhcvohih7ut>;
        "Balances": Anonymize<Idj13i7adlomht>;
        "Sudo": Anonymize<Iaug04qjhbli00>;
        "SessionCommitteeManagement": Anonymize<I74qlsb663rkj5>;
        "RuntimeUpgrade": Anonymize<I9pu4hgnkc6uhr>;
        "NodeVersion": undefined;
        "NativeTokenManagement": Anonymize<I2vfjmnfdj2sqr>;
        "NativeTokenObservation": Anonymize<I4jrglhf7ll5q8>;
        "Preimage": Anonymize<I4cfhml1prt4lu>;
        "MultiBlockMigrations": Anonymize<Iaaqq5jevtahm8>;
        "PalletSession": Anonymize<I1e07dgbaqd1sq>;
        "Scheduler": Anonymize<If7oa8fprnilo5>;
        "TxPause": Anonymize<Ifku1elmu8hk3i>;
        "Beefy": Anonymize<Iflve6qd33ah68>;
        "Mmr": undefined;
        "BeefyMmrLeaf": undefined;
        "Session": undefined;
        "GovernedMap": Anonymize<Icvl09463dsu5e>;
    }>;
    "ConsumerRemaining": undefined;
    "NoProviders": undefined;
    "TooManyConsumers": undefined;
    "Token": TokenError;
    "Arithmetic": ArithmeticError;
    "Transactional": TransactionalError;
    "Exhausted": undefined;
    "Corruption": undefined;
    "Unavailable": undefined;
    "RootNotAllowed": undefined;
    "Trie": Enum<{
        "InvalidStateRoot": undefined;
        "IncompleteDatabase": undefined;
        "ValueAtIncompleteKey": undefined;
        "DecoderError": undefined;
        "InvalidHash": undefined;
        "DuplicateKey": undefined;
        "ExtraneousNode": undefined;
        "ExtraneousValue": undefined;
        "ExtraneousHashReference": undefined;
        "InvalidChildReference": undefined;
        "ValueMismatch": undefined;
        "IncompleteProof": undefined;
        "RootMismatch": undefined;
        "DecodeError": undefined;
    }>;
}>;
export type I5o0s7c8q1cc9b = AnonymousEnum<{
    /**
     * The name of specification does not match between the current runtime
     * and the new runtime.
     */
    "InvalidSpecName": undefined;
    /**
     * The specification version is not allowed to decrease between the current runtime
     * and the new runtime.
     */
    "SpecVersionNeedsToIncrease": undefined;
    /**
     * Failed to extract the runtime version from the new runtime.
     *
     * Either calling `Core_version` or decoding `RuntimeVersion` failed.
     */
    "FailedToExtractRuntimeVersion": undefined;
    /**
     * Suicide called when the account has non-default composite data.
     */
    "NonDefaultComposite": undefined;
    /**
     * There is a non-zero reference count preventing the account from being purged.
     */
    "NonZeroRefCount": undefined;
    /**
     * The origin filter prevent the call to be dispatched.
     */
    "CallFiltered": undefined;
    /**
     * A multi-block migration is ongoing and prevents the current code from being replaced.
     */
    "MultiBlockMigrationsOngoing": undefined;
    /**
     * No upgrade authorized.
     */
    "NothingAuthorized": undefined;
    /**
     * The submitted code is not authorized.
     */
    "Unauthorized": undefined;
}>;
export type I7q8i0pp1gkas6 = AnonymousEnum<{
    /**
     * Attempt to signal GRANDPA pause when the authority set isn't live
     * (either paused or already pending pause).
     */
    "PauseFailed": undefined;
    /**
     * Attempt to signal GRANDPA resume when the authority set isn't paused
     * (either live or already pending resume).
     */
    "ResumeFailed": undefined;
    /**
     * Attempt to signal GRANDPA change with one already pending.
     */
    "ChangePending": undefined;
    /**
     * Cannot signal forced change so soon after last.
     */
    "TooSoon": undefined;
    /**
     * A key ownership proof provided as part of an equivocation report is invalid.
     */
    "InvalidKeyOwnershipProof": undefined;
    /**
     * An equivocation proof provided as part of an equivocation report is invalid.
     */
    "InvalidEquivocationProof": undefined;
    /**
     * A given equivocation report is valid but already previously reported.
     */
    "DuplicateOffenceReport": undefined;
}>;
export type I6nhhcvohih7ut = AnonymousEnum<{
    "NewStateOutOfBounds": undefined;
    "Deserialization": Anonymize<Ihhhb06ltk59c>;
    "Serialization": Anonymize<Ichp8s4hhgp7ug>;
    "Transaction": Anonymize<Ic4hvjmrliv95i>;
    "LedgerCacheError": undefined;
    "NoLedgerState": undefined;
    "LedgerStateScaleDecodingError": undefined;
    "ContractCallCostError": undefined;
}>;
export type Ihhhb06ltk59c = AnonymousEnum<{
    "NetworkId": undefined;
    "Transaction": undefined;
    "LedgerState": undefined;
    "ContractAddress": undefined;
    "PublicKey": undefined;
    "VersionedArenaKey": undefined;
    "UserAddress": undefined;
}>;
export type Ichp8s4hhgp7ug = AnonymousEnum<{
    "TransactionIdentifier": undefined;
    "ZswapState": undefined;
    "LedgerState": undefined;
    "LedgerParameters": undefined;
    "ContractAddress": undefined;
    "ContractState": undefined;
    "ContractStateToJson": undefined;
    "UnknownType": undefined;
    "MerkleTreeDigest": undefined;
    "VersionedArenaKey": undefined;
}>;
export type Ic4hvjmrliv95i = AnonymousEnum<{
    "Invalid": Enum<{
        "EffectsMismatch": undefined;
        "ContractAlreadyDeployed": undefined;
        "ContractNotPresent": undefined;
        "Zswap": undefined;
        "Transcript": undefined;
        "InsufficientClaimable": undefined;
        "VerifierKeyNotFound": undefined;
        "VerifierKeyAlreadyPresent": undefined;
        "ReplayCounterMismatch": undefined;
        "UnknownError": undefined;
    }>;
    "Malformed": Enum<{
        "VerifierKeyNotSet": undefined;
        "TransactionTooLarge": undefined;
        "VerifierKeyTooLarge": undefined;
        "VerifierKeyNotPresent": undefined;
        "ContractNotPresent": undefined;
        "InvalidProof": undefined;
        "BindingCommitmentOpeningInvalid": undefined;
        "NotNormalized": undefined;
        "FallibleWithoutCheckpoint": undefined;
        "ClaimReceiveFailed": undefined;
        "ClaimSpendFailed": undefined;
        "ClaimNullifierFailed": undefined;
        "ClaimCallFailed": undefined;
        "InvalidSchnorrProof": undefined;
        "UnclaimedCoinCom": undefined;
        "UnclaimedNullifier": undefined;
        "Unbalanced": undefined;
        "Zswap": undefined;
        "BuiltinDecode": undefined;
        "GuaranteedLimit": undefined;
        "MergingContracts": undefined;
        "CantMergeTypes": undefined;
        "ClaimOverflow": undefined;
        "ClaimCoinMismatch": undefined;
        "KeyNotInCommittee": undefined;
        "InvalidCommitteeSignature": undefined;
        "ThresholdMissed": undefined;
        "TooManyZswapEntries": undefined;
        "UnknownError": undefined;
    }>;
    "SystemTransaction": Enum<{
        "IllegalMint": undefined;
        "InsufficientTreasuryFunds": undefined;
        "CommitmentAlreadyPresent": undefined;
        "UnknownError": undefined;
        "ReplayProtectionFailure": undefined;
    }>;
}>;
export type Idj13i7adlomht = AnonymousEnum<{
    /**
     * Vesting balance too high to send value.
     */
    "VestingBalance": undefined;
    /**
     * Account liquidity restrictions prevent withdrawal.
     */
    "LiquidityRestrictions": undefined;
    /**
     * Balance too low to send value.
     */
    "InsufficientBalance": undefined;
    /**
     * Value too low to create account due to existential deposit.
     */
    "ExistentialDeposit": undefined;
    /**
     * Transfer/payment would kill account.
     */
    "Expendability": undefined;
    /**
     * A vesting schedule already exists for this account.
     */
    "ExistingVestingSchedule": undefined;
    /**
     * Beneficiary account must pre-exist.
     */
    "DeadAccount": undefined;
    /**
     * Number of named reserves exceed `MaxReserves`.
     */
    "TooManyReserves": undefined;
    /**
     * Number of holds exceed `VariantCountOf<T::RuntimeHoldReason>`.
     */
    "TooManyHolds": undefined;
    /**
     * Number of freezes exceed `MaxFreezes`.
     */
    "TooManyFreezes": undefined;
    /**
     * The issuance cannot be modified since it is already deactivated.
     */
    "IssuanceDeactivated": undefined;
    /**
     * The delta cannot be zero.
     */
    "DeltaZero": undefined;
}>;
export type Iaug04qjhbli00 = AnonymousEnum<{
    /**
     * Sender must be the Sudo account.
     */
    "RequireSudo": undefined;
}>;
export type I74qlsb663rkj5 = AnonymousEnum<{
    /**
     * [Pallet::set] has been called with epoch number that is not current epoch + 1
     */
    "InvalidEpoch": undefined;
    /**
     * [Pallet::set] has been called a second time for the same next epoch
     */
    "NextCommitteeAlreadySet": undefined;
}>;
export type I9pu4hgnkc6uhr = AnonymousEnum<{
    /**
     * Inherent transaction requires current authority information, but this was not able to be retrived from AURA
     */
    "CouldNotLoadCurrentAuthority": undefined;
    /**
     * An error occurred when calling a runtime upgrade
     */
    "RuntimeUpgradeError": undefined;
    /**
     * Limit for votes was exceeded
     */
    "VoteThresholdExceeded": undefined;
}>;
export type I2vfjmnfdj2sqr = AnonymousEnum<{
    /**
     * Indicates that the inherent was called while there was no main chain scripts set in the
     * pallet's storage. This is indicative of a programming bug.
     */
    "CalledWithoutConfiguration": undefined;
    /**
     * Indicates that the inherent was called a second time in the same block
     */
    "TransferAlreadyHandled": undefined;
}>;
export type I4jrglhf7ll5q8 = AnonymousEnum<{
    /**
     * A Cardano Wallet address was sent, but was longer than expected
     */
    "MaxCardanoAddrLengthExceeded": undefined;
    "MaxRegistrationsExceeded": undefined;
}>;
export type I4cfhml1prt4lu = AnonymousEnum<{
    /**
     * Preimage is too large to store on-chain.
     */
    "TooBig": undefined;
    /**
     * Preimage has already been noted on-chain.
     */
    "AlreadyNoted": undefined;
    /**
     * The user is not authorized to perform this action.
     */
    "NotAuthorized": undefined;
    /**
     * The preimage cannot be removed since it has not yet been noted.
     */
    "NotNoted": undefined;
    /**
     * A preimage may not be removed when there are outstanding requests.
     */
    "Requested": undefined;
    /**
     * The preimage request cannot be removed since no outstanding requests exist.
     */
    "NotRequested": undefined;
    /**
     * More than `MAX_HASH_UPGRADE_BULK_COUNT` hashes were requested to be upgraded at once.
     */
    "TooMany": undefined;
    /**
     * Too few hashes were requested to be upgraded (i.e. zero).
     */
    "TooFew": undefined;
}>;
export type Iaaqq5jevtahm8 = AnonymousEnum<{
    /**
     * The operation cannot complete since some MBMs are ongoing.
     */
    "Ongoing": undefined;
}>;
export type I1e07dgbaqd1sq = AnonymousEnum<{
    /**
     * Invalid ownership proof.
     */
    "InvalidProof": undefined;
    /**
     * No associated validator ID for account.
     */
    "NoAssociatedValidatorId": undefined;
    /**
     * Registered duplicate key.
     */
    "DuplicatedKey": undefined;
    /**
     * No keys are associated with this account.
     */
    "NoKeys": undefined;
    /**
     * Key setting account is not live, so it's impossible to associate keys.
     */
    "NoAccount": undefined;
}>;
export type If7oa8fprnilo5 = AnonymousEnum<{
    /**
     * Failed to schedule a call
     */
    "FailedToSchedule": undefined;
    /**
     * Cannot find the scheduled call.
     */
    "NotFound": undefined;
    /**
     * Given target block number is in the past.
     */
    "TargetBlockNumberInPast": undefined;
    /**
     * Reschedule failed because it does not change scheduled time.
     */
    "RescheduleNoChange": undefined;
    /**
     * Attempt to use a non-named function on a named task.
     */
    "Named": undefined;
}>;
export type Ifku1elmu8hk3i = AnonymousEnum<{
    /**
     * The call is paused.
     */
    "IsPaused": undefined;
    /**
     * The call is unpaused.
     */
    "IsUnpaused": undefined;
    /**
     * The call is whitelisted and cannot be paused.
     */
    "Unpausable": undefined;
    "NotFound": undefined;
}>;
export type Iflve6qd33ah68 = AnonymousEnum<{
    /**
     * A key ownership proof provided as part of an equivocation report is invalid.
     */
    "InvalidKeyOwnershipProof": undefined;
    /**
     * A double voting proof provided as part of an equivocation report is invalid.
     */
    "InvalidDoubleVotingProof": undefined;
    /**
     * A fork voting proof provided as part of an equivocation report is invalid.
     */
    "InvalidForkVotingProof": undefined;
    /**
     * A future block voting proof provided as part of an equivocation report is invalid.
     */
    "InvalidFutureBlockVotingProof": undefined;
    /**
     * The session of the equivocation proof is invalid
     */
    "InvalidEquivocationProofSession": undefined;
    /**
     * A given equivocation report is valid but already previously reported.
     */
    "DuplicateOffenceReport": undefined;
    /**
     * Submitted configuration is invalid.
     */
    "InvalidConfiguration": undefined;
}>;
export type Icvl09463dsu5e = AnonymousEnum<{
    /**
     * Signals that the inherent has been called again in the same block
     */
    "InherentCalledTwice": undefined;
    /**
     * MainChainScript is not set, registration of changes is not allowed
     */
    "MainChainScriptNotSet": undefined;
}>;
export type TokenError = Enum<{
    "FundsUnavailable": undefined;
    "OnlyProvider": undefined;
    "BelowMinimum": undefined;
    "CannotCreate": undefined;
    "UnknownAsset": undefined;
    "Frozen": undefined;
    "Unsupported": undefined;
    "CannotCreateHold": undefined;
    "NotExpendable": undefined;
    "Blocked": undefined;
}>;
export declare const TokenError: GetEnum<TokenError>;
export type ArithmeticError = Enum<{
    "Underflow": undefined;
    "Overflow": undefined;
    "DivisionByZero": undefined;
}>;
export declare const ArithmeticError: GetEnum<ArithmeticError>;
export type TransactionalError = Enum<{
    "LimitReached": undefined;
    "NoLayer": undefined;
}>;
export declare const TransactionalError: GetEnum<TransactionalError>;
export type Icbccs0ug47ilf = {
    "account": SS58String;
};
export type I855j4i3kr8ko1 = {
    "sender": SS58String;
    "hash": FixedSizeBinary<32>;
};
export type Ibgl04rn6nbfm6 = {
    "code_hash": FixedSizeBinary<32>;
    "check_version": boolean;
};
export type I7f4oosp2hcgjo = {
    "code_hash": FixedSizeBinary<32>;
    "error": Anonymize<Im2qle9pka0f8>;
};
export type GrandpaEvent = Enum<{
    /**
     * New authority set has been applied.
     */
    "NewAuthorities": Anonymize<I5768ac424h061>;
    /**
     * Current authority set has been paused.
     */
    "Paused": undefined;
    /**
     * Current authority set has been resumed.
     */
    "Resumed": undefined;
}>;
export declare const GrandpaEvent: GetEnum<GrandpaEvent>;
export type I5768ac424h061 = {
    "authority_set": Anonymize<I3geksg000c171>;
};
export type I3geksg000c171 = Array<[FixedSizeBinary<32>, bigint]>;
export type I19tmqo032ll7v = AnonymousEnum<{
    /**
     * A contract was called.
     */
    "ContractCall": Anonymize<I5o0in87i4h9qh>;
    /**
     * A contract has been deployed.
     */
    "ContractDeploy": Anonymize<I5o0in87i4h9qh>;
    /**
     * A transaction has been applied (both the guaranteed and conditional part).
     */
    "TxApplied": FixedSizeBinary<32>;
    /**
     * Contract ownership changes to enable snark upgrades
     */
    "ContractMaintain": Anonymize<I5o0in87i4h9qh>;
    /**
     * New payout minted.
     */
    "PayoutMinted": Anonymize<I7nfq6ftas0rri>;
    /**
     * Payout was claimed.
     */
    "ClaimMint": Anonymize<I9qdvp794ab9dj>;
    /**
     * Unshielded Tokens Trasfers
     */
    "UnshieldedTokens": Anonymize<I3seth7anm0bu2>;
    /**
     * Partial Success.
     */
    "TxPartialSuccess": FixedSizeBinary<32>;
}>;
export type I5o0in87i4h9qh = {
    "tx_hash": FixedSizeBinary<32>;
    "contract_address": Binary;
};
export type I7nfq6ftas0rri = {
    "amount": bigint;
    "receiver": Binary;
};
export type I9qdvp794ab9dj = {
    "tx_hash": FixedSizeBinary<32>;
    "coin_type": FixedSizeBinary<32>;
    "value": bigint;
};
export type I3seth7anm0bu2 = {
    "spent": Array<{
        "address": FixedSizeBinary<32>;
        "token_type": FixedSizeBinary<32>;
        "intent_hash": FixedSizeBinary<32>;
        "value": bigint;
        "output_no": number;
    }>;
    "created": Array<{
        "address": FixedSizeBinary<32>;
        "token_type": FixedSizeBinary<32>;
        "intent_hash": FixedSizeBinary<32>;
        "value": bigint;
        "output_no": number;
    }>;
};
export type Iao8h4hv7atnq3 = AnonymousEnum<{
    /**
     * An account was created with some free balance.
     */
    "Endowed": Anonymize<Icv68aq8841478>;
    /**
     * An account was removed whose balance was non-zero but below ExistentialDeposit,
     * resulting in an outright loss.
     */
    "DustLost": Anonymize<Ic262ibdoec56a>;
    /**
     * Transfer succeeded.
     */
    "Transfer": Anonymize<Iflcfm9b6nlmdd>;
    /**
     * A balance was set by root.
     */
    "BalanceSet": Anonymize<Ijrsf4mnp3eka>;
    /**
     * Some balance was reserved (moved from free to reserved).
     */
    "Reserved": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some balance was unreserved (moved from reserved to free).
     */
    "Unreserved": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some balance was moved from the reserve of the first account to the second account.
     * Final argument indicates the destination balance type.
     */
    "ReserveRepatriated": Anonymize<I8tjvj9uq4b7hi>;
    /**
     * Some amount was deposited (e.g. for transaction fees).
     */
    "Deposit": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some amount was withdrawn from the account (e.g. for transaction fees).
     */
    "Withdraw": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some amount was removed from the account (e.g. for misbehavior).
     */
    "Slashed": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some amount was minted into an account.
     */
    "Minted": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some amount was burned from an account.
     */
    "Burned": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some amount was suspended from an account (it can be restored later).
     */
    "Suspended": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some amount was restored into an account.
     */
    "Restored": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * An account was upgraded.
     */
    "Upgraded": Anonymize<I4cbvqmqadhrea>;
    /**
     * Total issuance was increased by `amount`, creating a credit to be balanced.
     */
    "Issued": Anonymize<I3qt1hgg4djhgb>;
    /**
     * Total issuance was decreased by `amount`, creating a debt to be balanced.
     */
    "Rescinded": Anonymize<I3qt1hgg4djhgb>;
    /**
     * Some balance was locked.
     */
    "Locked": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some balance was unlocked.
     */
    "Unlocked": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some balance was frozen.
     */
    "Frozen": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * Some balance was thawed.
     */
    "Thawed": Anonymize<Id5fm4p8lj5qgi>;
    /**
     * The `TotalIssuance` was forcefully changed.
     */
    "TotalIssuanceForced": Anonymize<I4fooe9dun9o0t>;
}>;
export type Icv68aq8841478 = {
    "account": SS58String;
    "free_balance": bigint;
};
export type Ic262ibdoec56a = {
    "account": SS58String;
    "amount": bigint;
};
export type Iflcfm9b6nlmdd = {
    "from": SS58String;
    "to": SS58String;
    "amount": bigint;
};
export type Ijrsf4mnp3eka = {
    "who": SS58String;
    "free": bigint;
};
export type Id5fm4p8lj5qgi = {
    "who": SS58String;
    "amount": bigint;
};
export type I8tjvj9uq4b7hi = {
    "from": SS58String;
    "to": SS58String;
    "amount": bigint;
    "destination_status": BalanceStatus;
};
export type BalanceStatus = Enum<{
    "Free": undefined;
    "Reserved": undefined;
}>;
export declare const BalanceStatus: GetEnum<BalanceStatus>;
export type I4cbvqmqadhrea = {
    "who": SS58String;
};
export type I3qt1hgg4djhgb = {
    "amount": bigint;
};
export type I4fooe9dun9o0t = {
    "old": bigint;
    "new": bigint;
};
export type I375j12p03cl6h = AnonymousEnum<{
    /**
     * A sudo call just took place.
     */
    "Sudid": Anonymize<I4npgd22g8nrk7>;
    /**
     * The sudo key has been updated.
     */
    "KeyChanged": Anonymize<I5rtkmhm2dng4u>;
    /**
     * The key was permanently removed.
     */
    "KeyRemoved": undefined;
    /**
     * A [sudo_as](Pallet::sudo_as) call just took place.
     */
    "SudoAsDone": Anonymize<I4npgd22g8nrk7>;
}>;
export type I4npgd22g8nrk7 = {
    /**
     * The result of the call made by the sudo user.
     */
    "sudo_result": Anonymize<Icju1e6umh03pe>;
};
export type Icju1e6umh03pe = ResultPayload<undefined, Anonymize<Im2qle9pka0f8>>;
export type I5rtkmhm2dng4u = {
    /**
     * The old sudo key (if one was previously set).
     */
    "old"?: (SS58String) | undefined;
    /**
     * The new sudo key (if one was set).
     */
    "new": SS58String;
};
export type I3ol70vu5p7158 = AnonymousEnum<{
    /**
     * Signal an issue when attempting a runtime upgrade, in a context where pallet errors are not accessible
     */
    "CouldNotScheduleRuntimeUpgrade": Anonymize<I3f33lun6ld0ef>;
    /**
     * No votes were made this round
     */
    "NoVotes": undefined;
    /**
     * Code upgrade managed by this pallet was scheduled
     */
    "UpgradeScheduled": Anonymize<I70pruthef0ilh>;
    /**
     * Validators could not agree on an upgrade, and voting will be reset
     */
    "NoConsensusOnUpgrade": undefined;
    /**
     * Upgrade was not performed because a preimage of the upgrade request was not found
     */
    "NoUpgradePreimageMissing": Anonymize<I6tccms8877uoj>;
    /**
     * Upgrade was not performed because the request for its preimage was not found
     */
    "NoUpgradePreimageNotRequested": Anonymize<I6tccms8877uoj>;
    /**
     * An upgrade was attempted, but the call size exceeded the configured bounds
     */
    "UpgradeCallTooLarge": Anonymize<I3f33lun6ld0ef>;
    /**
     * A validator has voted on an upgrade
     */
    "Voted": Anonymize<I1ohl1lmd4r5j8>;
}>;
export type I3f33lun6ld0ef = {
    "runtime_hash": FixedSizeBinary<32>;
    "spec_version": number;
};
export type I70pruthef0ilh = {
    "runtime_hash": FixedSizeBinary<32>;
    "spec_version": number;
    "scheduled_for": number;
};
export type I6tccms8877uoj = {
    "preimage_hash": FixedSizeBinary<32>;
};
export type I1ohl1lmd4r5j8 = {
    "voter": FixedSizeBinary<32>;
    "target": Anonymize<I3f33lun6ld0ef>;
};
export type I3gp01gnl65els = AnonymousEnum<{
    /**
     * Signals that a new native token transfer has been processed by the pallet
     */
    "TokensTransfered": bigint;
}>;
export type Idrsj91vub2dkq = AnonymousEnum<{
    "Added": Anonymize<I7dt739idcq9qk>;
    /**
     * Tried to remove an element, but it was not found in the list of registrations
     */
    "AttemptedRemoveNonexistantElement": Anonymize<Ic1blifjtonodb>;
    /**
     * Could not add registration
     */
    "CouldNotAddRegistration": undefined;
    "DuplicatedRegistration": Anonymize<I7dt739idcq9qk>;
    "InvalidCardanoAddress": undefined;
    "InvalidDustAddress": undefined;
    "Registered": Anonymize<I7dt739idcq9qk>;
    /**
     * Removed registrations
     */
    "Removed": Anonymize<I4nibra9bv0ahp>;
    /**
     * Removed single registration in order to add a new registration in order to respect length bound of registration list
     */
    "RemovedOld": Anonymize<I4nibra9bv0ahp>;
    /**
     * System transaction - the `SystemTx` struct is defined in the Node for now, but this event will contain a Ledger System Transaction
     */
    "SystemTx": Anonymize<I9i3o8bp584ej>;
}>;
export type I7dt739idcq9qk = [Binary, Anonymize<I1am2l3mod97ls>];
export type I1am2l3mod97ls = Array<Anonymize<Ic1blifjtonodb>>;
export type Ic1blifjtonodb = {
    "cardano_address": Binary;
    "dust_address": Binary;
    "utxo_id": FixedSizeBinary<32>;
    "utxo_index": number;
};
export type I4nibra9bv0ahp = [Binary, Anonymize<Ic1blifjtonodb>];
export type I9i3o8bp584ej = {
    "header": {
        "block_hash": FixedSizeBinary<32>;
        "tx_index_in_block": number;
    };
    "body": Array<{
        "value": bigint;
        "owner": Binary;
        "time": bigint;
        "action": Enum<{
            "Create": undefined;
            "Destroy": undefined;
        }>;
        "nonce": FixedSizeBinary<32>;
    }>;
};
export type PreimageEvent = Enum<{
    /**
     * A preimage has been noted.
     */
    "Noted": Anonymize<I1jm8m1rh9e20v>;
    /**
     * A preimage has been requested.
     */
    "Requested": Anonymize<I1jm8m1rh9e20v>;
    /**
     * A preimage has ben cleared.
     */
    "Cleared": Anonymize<I1jm8m1rh9e20v>;
}>;
export declare const PreimageEvent: GetEnum<PreimageEvent>;
export type I1jm8m1rh9e20v = {
    "hash": FixedSizeBinary<32>;
};
export type I94co7vj7h6bo = AnonymousEnum<{
    /**
     * A Runtime upgrade started.
     *
     * Its end is indicated by `UpgradeCompleted` or `UpgradeFailed`.
     */
    "UpgradeStarted": Anonymize<If1co0pilmi7oq>;
    /**
     * The current runtime upgrade completed.
     *
     * This implies that all of its migrations completed successfully as well.
     */
    "UpgradeCompleted": undefined;
    /**
     * Runtime upgrade failed.
     *
     * This is very bad and will require governance intervention.
     */
    "UpgradeFailed": undefined;
    /**
     * A migration was skipped since it was already executed in the past.
     */
    "MigrationSkipped": Anonymize<I666bl2fqjkejo>;
    /**
     * A migration progressed.
     */
    "MigrationAdvanced": Anonymize<Iae74gjak1qibn>;
    /**
     * A Migration completed.
     */
    "MigrationCompleted": Anonymize<Iae74gjak1qibn>;
    /**
     * A Migration failed.
     *
     * This implies that the whole upgrade failed and governance intervention is required.
     */
    "MigrationFailed": Anonymize<Iae74gjak1qibn>;
    /**
     * The set of historical migrations has been cleared.
     */
    "HistoricCleared": Anonymize<I3escdojpj0551>;
}>;
export type If1co0pilmi7oq = {
    /**
     * The number of migrations that this upgrade contains.
     *
     * This can be used to design a progress indicator in combination with counting the
     * `MigrationCompleted` and `MigrationSkipped` events.
     */
    "migrations": number;
};
export type I666bl2fqjkejo = {
    /**
     * The index of the skipped migration within the [`Config::Migrations`] list.
     */
    "index": number;
};
export type Iae74gjak1qibn = {
    /**
     * The index of the migration within the [`Config::Migrations`] list.
     */
    "index": number;
    /**
     * The number of blocks that this migration took so far.
     */
    "took": number;
};
export type I3escdojpj0551 = {
    /**
     * Should be passed to `clear_historic` in a successive call.
     */
    "next_cursor"?: Anonymize<Iabpgqcjikia83>;
};
export type Iabpgqcjikia83 = (Binary) | undefined;
export type I4co4bgsti676q = AnonymousEnum<{
    /**
     * New session has happened. Note that the argument is the session index, not the
     * block number as the type might suggest.
     */
    "NewSession": Anonymize<I2hq50pu2kdjpo>;
    /**
     * Validator has been disabled.
     */
    "ValidatorDisabled": Anonymize<I9acqruh7322g2>;
    /**
     * Validator has been re-enabled.
     */
    "ValidatorReenabled": Anonymize<I9acqruh7322g2>;
}>;
export type I2hq50pu2kdjpo = {
    "session_index": number;
};
export type I9acqruh7322g2 = {
    "validator": SS58String;
};
export type Ienmbdqscc5642 = AnonymousEnum<{
    /**
     * Scheduled some task.
     */
    "Scheduled": Anonymize<I5n4sebgkfr760>;
    /**
     * Canceled some task.
     */
    "Canceled": Anonymize<I5n4sebgkfr760>;
    /**
     * Dispatched some task.
     */
    "Dispatched": Anonymize<Ifhslud4mm5dob>;
    /**
     * Set a retry configuration for some task.
     */
    "RetrySet": Anonymize<Ia3c82eadg79bj>;
    /**
     * Cancel a retry configuration for some task.
     */
    "RetryCancelled": Anonymize<Ienusoeb625ftq>;
    /**
     * The call for the provided hash was not found so the task has been aborted.
     */
    "CallUnavailable": Anonymize<Ienusoeb625ftq>;
    /**
     * The given task was unable to be renewed since the agenda is full at that block.
     */
    "PeriodicFailed": Anonymize<Ienusoeb625ftq>;
    /**
     * The given task was unable to be retried since the agenda is full at that block or there
     * was not enough weight to reschedule it.
     */
    "RetryFailed": Anonymize<Ienusoeb625ftq>;
    /**
     * The given task can never be executed since it is overweight.
     */
    "PermanentlyOverweight": Anonymize<Ienusoeb625ftq>;
    /**
     * Agenda is incomplete from `when`.
     */
    "AgendaIncomplete": Anonymize<Ibtsa3docbr9el>;
}>;
export type I5n4sebgkfr760 = {
    "when": number;
    "index": number;
};
export type Ifhslud4mm5dob = {
    "task": Anonymize<I9jd27rnpm8ttv>;
    "id"?: Anonymize<I4s6vifaf8k998>;
    "result": Anonymize<Icju1e6umh03pe>;
};
export type I9jd27rnpm8ttv = FixedSizeArray<2, number>;
export type I4s6vifaf8k998 = (FixedSizeBinary<32>) | undefined;
export type Ia3c82eadg79bj = {
    "task": Anonymize<I9jd27rnpm8ttv>;
    "id"?: Anonymize<I4s6vifaf8k998>;
    "period": number;
    "retries": number;
};
export type Ienusoeb625ftq = {
    "task": Anonymize<I9jd27rnpm8ttv>;
    "id"?: Anonymize<I4s6vifaf8k998>;
};
export type Ibtsa3docbr9el = {
    "when": number;
};
export type I9ulgod11dfvq5 = AnonymousEnum<{
    /**
     * This pallet, or a specific call is now paused.
     */
    "CallPaused": Anonymize<Iba7pefg0d11kh>;
    /**
     * This pallet, or a specific call is now unpaused.
     */
    "CallUnpaused": Anonymize<Iba7pefg0d11kh>;
}>;
export type Iba7pefg0d11kh = {
    "full_name": Anonymize<Idkbvh6dahk1v7>;
};
export type Idkbvh6dahk1v7 = FixedSizeArray<2, Binary>;
export type SessionEvent = Enum<{
    /**
     * New session has happened. Note that the argument is the session index, not the
     * block number as the type might suggest.
     */
    "NewSession": Anonymize<I2hq50pu2kdjpo>;
}>;
export declare const SessionEvent: GetEnum<SessionEvent>;
export type Ic5m5lp1oioo8r = Array<FixedSizeBinary<32>>;
export type I95g6i7ilua7lq = Array<Anonymize<I9jd27rnpm8ttv>>;
export type Ieniouoqkq4icf = {
    "spec_version": number;
    "spec_name": string;
};
export type GrandpaStoredState = Enum<{
    "Live": undefined;
    "PendingPause": {
        "scheduled_at": number;
        "delay": number;
    };
    "Paused": undefined;
    "PendingResume": {
        "scheduled_at": number;
        "delay": number;
    };
}>;
export declare const GrandpaStoredState: GetEnum<GrandpaStoredState>;
export type I7pe2me3i3vtn9 = {
    "scheduled_at": number;
    "delay": number;
    "next_authorities": Anonymize<I3geksg000c171>;
    "forced"?: Anonymize<I4arjljr6dpflb>;
};
export type I4arjljr6dpflb = (number) | undefined;
export type Ib7m93p5rn57dr = {
    "tx_hash": FixedSizeBinary<32>;
    "index": number;
};
export type I8ds64oj6581v0 = Array<{
    "id": FixedSizeBinary<8>;
    "amount": bigint;
    "reasons": BalancesTypesReasons;
}>;
export type BalancesTypesReasons = Enum<{
    "Fee": undefined;
    "Misc": undefined;
    "All": undefined;
}>;
export declare const BalancesTypesReasons: GetEnum<BalancesTypesReasons>;
export type Ia7pdug7cdsg8g = Array<{
    "id": FixedSizeBinary<8>;
    "amount": bigint;
}>;
export type I9bin2jc70qt6q = Array<Anonymize<I3qt1hgg4djhgb>>;
export type I9dgeh47ldhst1 = {
    "epoch": bigint;
    "committee": Anonymize<I7tbts2dcf9pg2>;
};
export type I7tbts2dcf9pg2 = Array<Enum<{
    "Permissioned": {
        "id": FixedSizeBinary<33>;
        "keys": Anonymize<I2p9qi8l69c2sq>;
    };
    "Registered": {
        "id": FixedSizeBinary<33>;
        "keys": Anonymize<I2p9qi8l69c2sq>;
        "stake_pool_pub_key": FixedSizeBinary<32>;
    };
}>>;
export type I2p9qi8l69c2sq = {
    "aura": FixedSizeBinary<32>;
    "grandpa": FixedSizeBinary<32>;
};
export type I4r5bhov0m7jqr = {
    "committee_candidate_address": Binary;
    "d_parameter_policy_id": FixedSizeBinary<28>;
    "permissioned_candidates_policy_id": FixedSizeBinary<28>;
};
export type I85ci61lv50332 = Array<[Anonymize<I3f33lun6ld0ef>, number]>;
export type I2t1vhi8pcujcb = {
    "native_token_policy_id": FixedSizeBinary<28>;
    "native_token_asset_name": Binary;
    "illiquid_supply_validator_address": Binary;
};
export type I6l15kir0trh70 = {
    "block_hash": FixedSizeBinary<32>;
    "block_number": number;
    "tx_index_in_block": number;
};
export type PreimageOldRequestStatus = Enum<{
    "Unrequested": {
        "deposit": Anonymize<I95l2k9b1re95f>;
        "len": number;
    };
    "Requested": {
        "deposit"?: (Anonymize<I95l2k9b1re95f>) | undefined;
        "count": number;
        "len"?: Anonymize<I4arjljr6dpflb>;
    };
}>;
export declare const PreimageOldRequestStatus: GetEnum<PreimageOldRequestStatus>;
export type I95l2k9b1re95f = [SS58String, bigint];
export type I8j24837rs9r0t = AnonymousEnum<{
    "Unrequested": {
        "ticket": Anonymize<Ifvqn3ldat80ai>;
        "len": number;
    };
    "Requested": {
        "maybe_ticket"?: (Anonymize<Ifvqn3ldat80ai>) | undefined;
        "count": number;
        "maybe_len"?: Anonymize<I4arjljr6dpflb>;
    };
}>;
export type Ifvqn3ldat80ai = [SS58String, undefined];
export type I4pact7n2e9a0i = [FixedSizeBinary<32>, number];
export type Iepbsvlk3qceij = AnonymousEnum<{
    "Active": {
        "index": number;
        "inner_cursor"?: Anonymize<Iabpgqcjikia83>;
        "started_at": number;
    };
    "Stuck": undefined;
}>;
export type Ia2lhg7l2hilo3 = Array<SS58String>;
export type Ibslbpu3d5lodd = Array<[SS58String, Anonymize<I2p9qi8l69c2sq>]>;
export type If7hhl1u9dung8 = Array<({
    "maybe_id"?: Anonymize<I4s6vifaf8k998>;
    "priority": number;
    "call": PreimagesBounded;
    "maybe_periodic"?: Anonymize<Iep7au1720bm0e>;
    "origin": Enum<{
        "system": DispatchRawOrigin;
    }>;
}) | undefined>;
export type PreimagesBounded = Enum<{
    "Legacy": Anonymize<I1jm8m1rh9e20v>;
    "Inline": Binary;
    "Lookup": {
        "hash": FixedSizeBinary<32>;
        "len": number;
    };
}>;
export declare const PreimagesBounded: GetEnum<PreimagesBounded>;
export type Iep7au1720bm0e = (Anonymize<I9jd27rnpm8ttv>) | undefined;
export type DispatchRawOrigin = Enum<{
    "Root": undefined;
    "Signed": SS58String;
    "None": undefined;
}>;
export declare const DispatchRawOrigin: GetEnum<DispatchRawOrigin>;
export type I56u24ncejr5kt = {
    "total_retries": number;
    "remaining": number;
    "period": number;
};
export type I2fb54desdqd9n = Array<FixedSizeBinary<33>>;
export type Idjett00s2gd = {
    "id": bigint;
    "len": number;
    "keyset_commitment": FixedSizeBinary<32>;
};
export type Icgljjb6j82uhn = Array<number>;
export type I2ho2o2f1oad8v = {
    "validator_address": Binary;
    "policy_id": FixedSizeBinary<28>;
};
export type In7a38730s6qs = {
    "base_block": Anonymize<I4q39t5hn830vp>;
    "max_block": Anonymize<I4q39t5hn830vp>;
    "per_class": {
        "normal": {
            "base_extrinsic": Anonymize<I4q39t5hn830vp>;
            "max_extrinsic"?: (Anonymize<I4q39t5hn830vp>) | undefined;
            "max_total"?: (Anonymize<I4q39t5hn830vp>) | undefined;
            "reserved"?: (Anonymize<I4q39t5hn830vp>) | undefined;
        };
        "operational": {
            "base_extrinsic": Anonymize<I4q39t5hn830vp>;
            "max_extrinsic"?: (Anonymize<I4q39t5hn830vp>) | undefined;
            "max_total"?: (Anonymize<I4q39t5hn830vp>) | undefined;
            "reserved"?: (Anonymize<I4q39t5hn830vp>) | undefined;
        };
        "mandatory": {
            "base_extrinsic": Anonymize<I4q39t5hn830vp>;
            "max_extrinsic"?: (Anonymize<I4q39t5hn830vp>) | undefined;
            "max_total"?: (Anonymize<I4q39t5hn830vp>) | undefined;
            "reserved"?: (Anonymize<I4q39t5hn830vp>) | undefined;
        };
    };
};
export type If15el53dd76v9 = {
    "normal": number;
    "operational": number;
    "mandatory": number;
};
export type I9s0ave7t0vnrk = {
    "read": bigint;
    "write": bigint;
};
export type I4fo08joqmcqnm = {
    "spec_name": string;
    "impl_name": string;
    "authoring_version": number;
    "spec_version": number;
    "impl_version": number;
    "apis": Array<[FixedSizeBinary<8>, number]>;
    "transaction_version": number;
    "system_version": number;
};
export type Iekve0i6djpd9f = AnonymousEnum<{
    /**
     * Make some on-chain remark.
     *
     * Can be executed by every `origin`.
     */
    "remark": Anonymize<I8ofcg5rbj0g2c>;
    /**
     * Set the number of pages in the WebAssembly environment's heap.
     */
    "set_heap_pages": Anonymize<I4adgbll7gku4i>;
    /**
     * Set the new runtime code.
     */
    "set_code": Anonymize<I6pjjpfvhvcfru>;
    /**
     * Set the new runtime code without doing any checks of the given `code`.
     *
     * Note that runtime upgrades will not run if this is called with a not-increasing spec
     * version!
     */
    "set_code_without_checks": Anonymize<I6pjjpfvhvcfru>;
    /**
     * Set some items of storage.
     */
    "set_storage": Anonymize<I9pj91mj79qekl>;
    /**
     * Kill some items from storage.
     */
    "kill_storage": Anonymize<I39uah9nss64h9>;
    /**
     * Kill all storage items with a key that starts with the given prefix.
     *
     * **NOTE:** We rely on the Root origin to provide us the number of subkeys under
     * the prefix we are removing to accurately calculate the weight of this function.
     */
    "kill_prefix": Anonymize<Ik64dknsq7k08>;
    /**
     * Make some on-chain remark and emit event.
     */
    "remark_with_event": Anonymize<I8ofcg5rbj0g2c>;
    /**
     * Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied
     * later.
     *
     * This call requires Root origin.
     */
    "authorize_upgrade": Anonymize<Ib51vk42m1po4n>;
    /**
     * Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied
     * later.
     *
     * WARNING: This authorizes an upgrade that will take place without any safety checks, for
     * example that the spec name remains the same and that the version number increases. Not
     * recommended for normal use. Use `authorize_upgrade` instead.
     *
     * This call requires Root origin.
     */
    "authorize_upgrade_without_checks": Anonymize<Ib51vk42m1po4n>;
    /**
     * Provide the preimage (runtime binary) `code` for an upgrade that has been authorized.
     *
     * If the authorization required a version check, this call will ensure the spec name
     * remains unchanged and that the spec version has increased.
     *
     * Depending on the runtime's `OnSetCode` configuration, this function may directly apply
     * the new `code` in the same block or attempt to schedule the upgrade.
     *
     * All origins are allowed.
     */
    "apply_authorized_upgrade": Anonymize<I6pjjpfvhvcfru>;
}>;
export type I8ofcg5rbj0g2c = {
    "remark": Binary;
};
export type I4adgbll7gku4i = {
    "pages": bigint;
};
export type I6pjjpfvhvcfru = {
    "code": Binary;
};
export type I9pj91mj79qekl = {
    "items": Array<Anonymize<Idkbvh6dahk1v7>>;
};
export type I39uah9nss64h9 = {
    "keys": Anonymize<Itom7fk49o0c9>;
};
export type Itom7fk49o0c9 = Array<Binary>;
export type Ik64dknsq7k08 = {
    "prefix": Binary;
    "subkeys": number;
};
export type Ib51vk42m1po4n = {
    "code_hash": FixedSizeBinary<32>;
};
export type I7d75gqfg6jh9c = AnonymousEnum<{
    /**
     * Set the current time.
     *
     * This call should be invoked exactly once per block. It will panic at the finalization
     * phase, if this call hasn't been invoked by that time.
     *
     * The timestamp should be greater than the previous one by the amount specified by
     * [`Config::MinimumPeriod`].
     *
     * The dispatch origin for this call must be _None_.
     *
     * This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware
     * that changing the complexity of this call could result exhausting the resources in a
     * block to execute any other calls.
     *
     * ## Complexity
     * - `O(1)` (Note that implementations of `OnTimestampSet` must also be `O(1)`)
     * - 1 storage read and 1 storage mutation (codec `O(1)` because of `DidUpdate::take` in
     * `on_finalize`)
     * - 1 event handler `on_timestamp_set`. Must be `O(1)`.
     */
    "set": Anonymize<Idcr6u6361oad9>;
}>;
export type Idcr6u6361oad9 = {
    "now": bigint;
};
export type Ibck9ekr2i96uj = AnonymousEnum<{
    /**
     * Report voter equivocation/misbehavior. This method will verify the
     * equivocation proof and validate the given key ownership proof
     * against the extracted offender. If both are valid, the offence
     * will be reported.
     */
    "report_equivocation": Anonymize<I3a5kuu5t5jj3g>;
    /**
     * Report voter equivocation/misbehavior. This method will verify the
     * equivocation proof and validate the given key ownership proof
     * against the extracted offender. If both are valid, the offence
     * will be reported.
     *
     * This extrinsic must be called unsigned and it is expected that only
     * block authors will call it (validated in `ValidateUnsigned`), as such
     * if the block author is defined it will be defined as the equivocation
     * reporter.
     */
    "report_equivocation_unsigned": Anonymize<I3a5kuu5t5jj3g>;
    /**
     * Note that the current authority set of the GRANDPA finality gadget has stalled.
     *
     * This will trigger a forced authority set change at the beginning of the next session, to
     * be enacted `delay` blocks after that. The `delay` should be high enough to safely assume
     * that the block signalling the forced change will not be re-orged e.g. 1000 blocks.
     * The block production rate (which may be slowed down because of finality lagging) should
     * be taken into account when choosing the `delay`. The GRANDPA voters based on the new
     * authority will start voting on top of `best_finalized_block_number` for new finalized
     * blocks. `best_finalized_block_number` should be the highest of the latest finalized
     * block of all validators of the new authority set.
     *
     * Only callable by root.
     */
    "note_stalled": Anonymize<I2hviml3snvhhn>;
}>;
export type I3a5kuu5t5jj3g = {
    "equivocation_proof": Anonymize<I9puqgoda8ofk4>;
};
export type I9puqgoda8ofk4 = {
    "set_id": bigint;
    "equivocation": GrandpaEquivocation;
};
export type GrandpaEquivocation = Enum<{
    "Prevote": {
        "round_number": bigint;
        "identity": FixedSizeBinary<32>;
        "first": [{
            "target_hash": FixedSizeBinary<32>;
            "target_number": number;
        }, FixedSizeBinary<64>];
        "second": [{
            "target_hash": FixedSizeBinary<32>;
            "target_number": number;
        }, FixedSizeBinary<64>];
    };
    "Precommit": {
        "round_number": bigint;
        "identity": FixedSizeBinary<32>;
        "first": [{
            "target_hash": FixedSizeBinary<32>;
            "target_number": number;
        }, FixedSizeBinary<64>];
        "second": [{
            "target_hash": FixedSizeBinary<32>;
            "target_number": number;
        }, FixedSizeBinary<64>];
    };
}>;
export declare const GrandpaEquivocation: GetEnum<GrandpaEquivocation>;
export type I2hviml3snvhhn = {
    "delay": number;
    "best_finalized_block_number": number;
};
export type I912mlc35loovi = AnonymousEnum<{
    "send_mn_transaction": Anonymize<Ifsi09mj2o3peu>;
    "set_mn_tx_weight": Anonymize<Iikug8jkjivr>;
    "override_d_parameter": Anonymize<Ib6fc72n7a066a>;
    "set_contract_call_weight": Anonymize<Iikug8jkjivr>;
    "set_tx_size_weight": Anonymize<Iikug8jkjivr>;
    "set_safe_mode": Anonymize<I5iilu2ehqsma0>;
}>;
export type Ifsi09mj2o3peu = {
    "midnight_tx": Binary;
};
export type Iikug8jkjivr = {
    "new_weight": Anonymize<I4q39t5hn830vp>;
};
export type Ib6fc72n7a066a = {
    "d_parameter_override"?: Anonymize<Iep7au1720bm0e>;
};
export type I5iilu2ehqsma0 = {
    "mode": boolean;
};
export type I9svldsp29mh87 = AnonymousEnum<{
    /**
     * Transfer some liquid free balance to another account.
     *
     * `transfer_allow_death` will set the `FreeBalance` of the sender and receiver.
     * If the sender's account is below the existential deposit as a result
     * of the transfer, the account will be reaped.
     *
     * The dispatch origin for this call must be `Signed` by the transactor.
     */
    "transfer_allow_death": Anonymize<I4ktuaksf5i1gk>;
    /**
     * Exactly as `transfer_allow_death`, except the origin must be root and the source account
     * may be specified.
     */
    "force_transfer": Anonymize<I9bqtpv2ii35mp>;
    /**
     * Same as the [`transfer_allow_death`] call, but with a check that the transfer will not
     * kill the origin account.
     *
     * 99% of the time you want [`transfer_allow_death`] instead.
     *
     * [`transfer_allow_death`]: struct.Pallet.html#method.transfer
     */
    "transfer_keep_alive": Anonymize<I4ktuaksf5i1gk>;
    /**
     * Transfer the entire transferable balance from the caller account.
     *
     * NOTE: This function only attempts to transfer _transferable_ balances. This means that
     * any locked, reserved, or existential deposits (when `keep_alive` is `true`), will not be
     * transferred by this function. To ensure that this function results in a killed account,
     * you might need to prepare the account by removing any reference counters, storage
     * deposits, etc...
     *
     * The dispatch origin of this call must be Signed.
     *
     * - `dest`: The recipient of the transfer.
     * - `keep_alive`: A boolean to determine if the `transfer_all` operation should send all
     * of the funds the account has, causing the sender account to be killed (false), or
     * transfer everything except at least the existential deposit, which will guarantee to
     * keep the sender account alive (true).
     */
    "transfer_all": Anonymize<I9j7pagd6d4bda>;
    /**
     * Unreserve some balance from a user by force.
     *
     * Can only be called by ROOT.
     */
    "force_unreserve": Anonymize<I2h9pmio37r7fb>;
    /**
     * Upgrade a specified account.
     *
     * - `origin`: Must be `Signed`.
     * - `who`: The account to be upgraded.
     *
     * This will waive the transaction fee if at least all but 10% of the accounts needed to
     * be upgraded. (We let some not have to be upgraded just in order to allow for the
     * possibility of churn).
     */
    "upgrade_accounts": Anonymize<Ibmr18suc9ikh9>;
    /**
     * Set the regular balance of a given account.
     *
     * The dispatch origin for this call is `root`.
     */
    "force_set_balance": Anonymize<I9iq22t0burs89>;
    /**
     * Adjust the total issuance in a saturating way.
     *
     * Can only be called by root and always needs a positive `delta`.
     *
     * # Example
     */
    "force_adjust_total_issuance": Anonymize<I5u8olqbbvfnvf>;
    /**
     * Burn the specified liquid free balance from the origin account.
     *
     * If the origin's account ends up below the existential deposit as a result
     * of the burn and `keep_alive` is false, the account will be reaped.
     *
     * Unlike sending funds to a _burn_ address, which merely makes the funds inaccessible,
     * this `burn` operation will reduce total issuance by the amount _burned_.
     */
    "burn": Anonymize<I5utcetro501ir>;
}>;
export type I4ktuaksf5i1gk = {
    "dest": MultiAddress;
    "value": bigint;
};
export type MultiAddress = Enum<{
    "Id": SS58String;
    "Index": undefined;
    "Raw": Binary;
    "Address32": FixedSizeBinary<32>;
    "Address20": FixedSizeBinary<20>;
}>;
export declare const MultiAddress: GetEnum<MultiAddress>;
export type I9bqtpv2ii35mp = {
    "source": MultiAddress;
    "dest": MultiAddress;
    "value": bigint;
};
export type I9j7pagd6d4bda = {
    "dest": MultiAddress;
    "keep_alive": boolean;
};
export type I2h9pmio37r7fb = {
    "who": MultiAddress;
    "amount": bigint;
};
export type Ibmr18suc9ikh9 = {
    "who": Anonymize<Ia2lhg7l2hilo3>;
};
export type I9iq22t0burs89 = {
    "who": MultiAddress;
    "new_free": bigint;
};
export type I5u8olqbbvfnvf = {
    "direction": BalancesAdjustmentDirection;
    "delta": bigint;
};
export type BalancesAdjustmentDirection = Enum<{
    "Increase": undefined;
    "Decrease": undefined;
}>;
export declare const BalancesAdjustmentDirection: GetEnum<BalancesAdjustmentDirection>;
export type I5utcetro501ir = {
    "value": bigint;
    "keep_alive": boolean;
};
export type Ido5lcb08rfkrb = AnonymousEnum<{
    /**
     * Authenticates the sudo key and dispatches a function call with `Root` origin.
     */
    "sudo": Anonymize<I6vutkc419j8di>;
    /**
     * Authenticates the sudo key and dispatches a function call with `Root` origin.
     * This function does not check the weight of the call, and instead allows the
     * Sudo user to specify the weight of the call.
     *
     * The dispatch origin for this call must be _Signed_.
     */
    "sudo_unchecked_weight": Anonymize<I4qt71sd8urp33>;
    /**
     * Authenticates the current sudo key and sets the given AccountId (`new`) as the new sudo
     * key.
     */
    "set_key": Anonymize<I8k3rnvpeeh4hv>;
    /**
     * Authenticates the sudo key and dispatches a function call with `Signed` origin from
     * a given account.
     *
     * The dispatch origin for this call must be _Signed_.
     */
    "sudo_as": Anonymize<Idqpf8qfqdr537>;
    /**
     * Permanently removes the sudo key.
     *
     * **This cannot be un-done.**
     */
    "remove_key": undefined;
}>;
export type I6vutkc419j8di = {
    "call": TxCallData;
};
export type I4qt71sd8urp33 = {
    "call": TxCallData;
    "weight": Anonymize<I4q39t5hn830vp>;
};
export type I8k3rnvpeeh4hv = {
    "new": MultiAddress;
};
export type Idqpf8qfqdr537 = {
    "who": MultiAddress;
    "call": TxCallData;
};
export type Ieu3guq2n6qufa = AnonymousEnum<{
    /**
     * 'for_epoch_number' parameter is needed only for validation purposes, because we need to make sure that
     * check_inherent uses the same epoch_number as was used to create inherent data.
     * Alternative approach would be to put epoch number inside InherentData. However, sidechain
     * epoch number is set in Runtime, thus, inherent data provider doesn't have to know about it.
     * On top of that, the latter approach is slightly more complicated to code.
     */
    "set": Anonymize<Iaer7i07e9es8i>;
    /**
     * Changes the main chain scripts used for committee rotation.
     *
     * This extrinsic must be run either using `sudo` or some other chain governance mechanism.
     */
    "set_main_chain_scripts": Anonymize<I4r5bhov0m7jqr>;
}>;
export type Iaer7i07e9es8i = {
    "validators": Anonymize<I7tbts2dcf9pg2>;
    "for_epoch_number": bigint;
    "selection_inputs_hash": FixedSizeBinary<32>;
};
export type I290u232f4cfcs = AnonymousEnum<{
    /**
     * Vote on a proposed runtime upgrade that is represented by an onchain preimage request
     *
     * This call should be invoked exactly once per block due to its inherent nature.
     *
     * The dispatch origin for this call must be _None_.
     *
     * This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware
     * that changing the complexity of this call could result exhausting the resources in a
     * block to execute any other calls.
     */
    "propose_or_vote_upgrade": Anonymize<I2hrprd0b1n00>;
}>;
export type I2hrprd0b1n00 = {
    "upgrade": Anonymize<I3f33lun6ld0ef>;
};
export type I2c42gmjg8ueu0 = AnonymousEnum<{
    /**
     * Inherent that registers new native token transfer from the Cardano main chain and triggers
     * the handler configured in [Config::TokenTransferHandler].
     *
     * Arguments:
     * - `token_amount`: the total amount of tokens transferred since the last invocation of the inherent
     */
    "transfer_tokens": Anonymize<I9lbuvnpos4dqh>;
    /**
     * Changes the main chain scripts used for observing native token transfers.
     *
     * This extrinsic must be run either using `sudo` or some other chain governance mechanism.
     */
    "set_main_chain_scripts": Anonymize<I2t1vhi8pcujcb>;
}>;
export type I9lbuvnpos4dqh = {
    "token_amount": bigint;
};
export type I7v9ukkd1fqtc7 = AnonymousEnum<{
    "process_tokens": Anonymize<I27f772v76dngh>;
    /**
     * Changes the mainchain address for the mapping validator contract
     *
     * This extrinsic must be run either using `sudo` or some other chain governance mechanism.
     */
    "set_mapping_validator_contract_address": Anonymize<Id7ndeh7rg6a8t>;
}>;
export type I27f772v76dngh = {
    "utxos": Array<{
        "header": {
            "tx_position": Anonymize<I6l15kir0trh70>;
            "tx_hash": FixedSizeBinary<32>;
            "utxo_tx_hash": FixedSizeBinary<32>;
            "utxo_index": number;
        };
        "data": Enum<{
            "Registration": {
                "cardano_address": Binary;
                "dust_address": Binary;
            };
            "Deregistration": {
                "cardano_address": Binary;
                "dust_address": Binary;
            };
            "AssetCreate": {
                "value": bigint;
                "owner": Binary;
                "utxo_tx_hash": FixedSizeBinary<32>;
                "utxo_tx_index": number;
            };
            "AssetSpend": {
                "value": bigint;
                "owner": Binary;
                "utxo_tx_hash": FixedSizeBinary<32>;
                "utxo_tx_index": number;
                "spending_tx_hash": FixedSizeBinary<32>;
            };
        }>;
    }>;
    "next_cardano_position": Anonymize<I6l15kir0trh70>;
};
export type Id7ndeh7rg6a8t = {
    "address": Binary;
};
export type If81ks88t5mpk5 = AnonymousEnum<{
    /**
     * Register a preimage on-chain.
     *
     * If the preimage was previously requested, no fees or deposits are taken for providing
     * the preimage. Otherwise, a deposit is taken proportional to the size of the preimage.
     */
    "note_preimage": Anonymize<I82nfqfkd48n10>;
    /**
     * Clear an unrequested preimage from the runtime storage.
     *
     * If `len` is provided, then it will be a much cheaper operation.
     *
     * - `hash`: The hash of the preimage to be removed from the store.
     * - `len`: The length of the preimage of `hash`.
     */
    "unnote_preimage": Anonymize<I1jm8m1rh9e20v>;
    /**
     * Request a preimage be uploaded to the chain without paying any fees or deposits.
     *
     * If the preimage requests has already been provided on-chain, we unreserve any deposit
     * a user may have paid, and take the control of the preimage out of their hands.
     */
    "request_preimage": Anonymize<I1jm8m1rh9e20v>;
    /**
     * Clear a previously made request for a preimage.
     *
     * NOTE: THIS MUST NOT BE CALLED ON `hash` MORE TIMES THAN `request_preimage`.
     */
    "unrequest_preimage": Anonymize<I1jm8m1rh9e20v>;
    /**
     * Ensure that the bulk of pre-images is upgraded.
     *
     * The caller pays no fee if at least 90% of pre-images were successfully updated.
     */
    "ensure_updated": Anonymize<I3o5j3bli1pd8e>;
}>;
export type I82nfqfkd48n10 = {
    "bytes": Binary;
};
export type I3o5j3bli1pd8e = {
    "hashes": Anonymize<Ic5m5lp1oioo8r>;
};
export type I4oqb168b2d4er = AnonymousEnum<{
    /**
     * Allows root to set a cursor to forcefully start, stop or forward the migration process.
     *
     * Should normally not be needed and is only in place as emergency measure. Note that
     * restarting the migration process in this manner will not call the
     * [`MigrationStatusHandler::started`] hook or emit an `UpgradeStarted` event.
     */
    "force_set_cursor": Anonymize<Ibou4u1engb441>;
    /**
     * Allows root to set an active cursor to forcefully start/forward the migration process.
     *
     * This is an edge-case version of [`Self::force_set_cursor`] that allows to set the
     * `started_at` value to the next block number. Otherwise this would not be possible, since
     * `force_set_cursor` takes an absolute block number. Setting `started_at` to `None`
     * indicates that the current block number plus one should be used.
     */
    "force_set_active_cursor": Anonymize<Id6nbvqoqdj4o2>;
    /**
     * Forces the onboarding of the migrations.
     *
     * This process happens automatically on a runtime upgrade. It is in place as an emergency
     * measurement. The cursor needs to be `None` for this to succeed.
     */
    "force_onboard_mbms": undefined;
    /**
     * Clears the `Historic` set.
     *
     * `map_cursor` must be set to the last value that was returned by the
     * `HistoricCleared` event. The first time `None` can be used. `limit` must be chosen in a
     * way that will result in a sensible weight.
     */
    "clear_historic": Anonymize<I95iqep3b8snn9>;
}>;
export type Ibou4u1engb441 = {
    "cursor"?: (Anonymize<Iepbsvlk3qceij>) | undefined;
};
export type Id6nbvqoqdj4o2 = {
    "index": number;
    "inner_cursor"?: Anonymize<Iabpgqcjikia83>;
    "started_at"?: Anonymize<I4arjljr6dpflb>;
};
export type I95iqep3b8snn9 = {
    "selector": Enum<{
        "Specific": Anonymize<Itom7fk49o0c9>;
        "Wildcard": {
            "limit"?: Anonymize<I4arjljr6dpflb>;
            "previous_cursor"?: Anonymize<Iabpgqcjikia83>;
        };
    }>;
};
export type I3vi1m70dcsram = AnonymousEnum<{
    /**
     * Sets the session key(s) of the function caller to `keys`.
     * Allows an account to set its session key prior to becoming a validator.
     * This doesn't take effect until the next session.
     *
     * The dispatch origin of this function must be signed.
     *
     * ## Complexity
     * - `O(1)`. Actual cost depends on the number of length of `T::Keys::key_ids()` which is
     * fixed.
     */
    "set_keys": Anonymize<Idk46ma354sc05>;
    /**
     * Removes any session key(s) of the function caller.
     *
     * This doesn't take effect until the next session.
     *
     * The dispatch origin of this function must be Signed and the account must be either be
     * convertible to a validator ID using the chain's typical addressing system (this usually
     * means being a controller account) or directly convertible into a validator ID (which
     * usually means being a stash account).
     *
     * ## Complexity
     * - `O(1)` in number of key types. Actual cost depends on the number of length of
     * `T::Keys::key_ids()` which is fixed.
     */
    "purge_keys": undefined;
}>;
export type Idk46ma354sc05 = {
    "keys": Anonymize<I2p9qi8l69c2sq>;
    "proof": Binary;
};
export type I6hbqiqupggq6o = AnonymousEnum<{
    /**
     * Anonymously schedule a task.
     */
    "schedule": Anonymize<If1jstp94m1o1a>;
    /**
     * Cancel an anonymously scheduled task.
     */
    "cancel": Anonymize<I5n4sebgkfr760>;
    /**
     * Schedule a named task.
     */
    "schedule_named": Anonymize<I31rh8oe57fm4v>;
    /**
     * Cancel a named scheduled task.
     */
    "cancel_named": Anonymize<Ifs1i5fk9cqvr6>;
    /**
     * Anonymously schedule a task after a delay.
     */
    "schedule_after": Anonymize<Ic4ko9p2tkh2jm>;
    /**
     * Schedule a named task after a delay.
     */
    "schedule_named_after": Anonymize<I2tkgcau710d91>;
    /**
     * Set a retry configuration for a task so that, in case its scheduled run fails, it will
     * be retried after `period` blocks, for a total amount of `retries` retries or until it
     * succeeds.
     *
     * Tasks which need to be scheduled for a retry are still subject to weight metering and
     * agenda space, same as a regular task. If a periodic task fails, it will be scheduled
     * normally while the task is retrying.
     *
     * Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic
     * clones of the original task. Their retry configuration will be derived from the
     * original task's configuration, but will have a lower value for `remaining` than the
     * original `total_retries`.
     */
    "set_retry": Anonymize<Ieg3fd8p4pkt10>;
    /**
     * Set a retry configuration for a named task so that, in case its scheduled run fails, it
     * will be retried after `period` blocks, for a total amount of `retries` retries or until
     * it succeeds.
     *
     * Tasks which need to be scheduled for a retry are still subject to weight metering and
     * agenda space, same as a regular task. If a periodic task fails, it will be scheduled
     * normally while the task is retrying.
     *
     * Tasks scheduled as a result of a retry for a periodic task are unnamed, non-periodic
     * clones of the original task. Their retry configuration will be derived from the
     * original task's configuration, but will have a lower value for `remaining` than the
     * original `total_retries`.
     */
    "set_retry_named": Anonymize<I8kg5ll427kfqq>;
    /**
     * Removes the retry configuration of a task.
     */
    "cancel_retry": Anonymize<I467333262q1l9>;
    /**
     * Cancel the retry configuration of a named task.
     */
    "cancel_retry_named": Anonymize<Ifs1i5fk9cqvr6>;
}>;
export type If1jstp94m1o1a = {
    "when": number;
    "maybe_periodic"?: Anonymize<Iep7au1720bm0e>;
    "priority": number;
    "call": TxCallData;
};
export type I31rh8oe57fm4v = {
    "id": FixedSizeBinary<32>;
    "when": number;
    "maybe_periodic"?: Anonymize<Iep7au1720bm0e>;
    "priority": number;
    "call": TxCallData;
};
export type Ifs1i5fk9cqvr6 = {
    "id": FixedSizeBinary<32>;
};
export type Ic4ko9p2tkh2jm = {
    "after": number;
    "maybe_periodic"?: Anonymize<Iep7au1720bm0e>;
    "priority": number;
    "call": TxCallData;
};
export type I2tkgcau710d91 = {
    "id": FixedSizeBinary<32>;
    "after": number;
    "maybe_periodic"?: Anonymize<Iep7au1720bm0e>;
    "priority": number;
    "call": TxCallData;
};
export type Ieg3fd8p4pkt10 = {
    "task": Anonymize<I9jd27rnpm8ttv>;
    "retries": number;
    "period": number;
};
export type I8kg5ll427kfqq = {
    "id": FixedSizeBinary<32>;
    "retries": number;
    "period": number;
};
export type I467333262q1l9 = {
    "task": Anonymize<I9jd27rnpm8ttv>;
};
export type Ieci88jft3cpv9 = AnonymousEnum<{
    /**
     * Pause a call.
     *
     * Can only be called by [`Config::PauseOrigin`].
     * Emits an [`Event::CallPaused`] event on success.
     */
    "pause": Anonymize<Iba7pefg0d11kh>;
    /**
     * Un-pause a call.
     *
     * Can only be called by [`Config::UnpauseOrigin`].
     * Emits an [`Event::CallUnpaused`] event on success.
     */
    "unpause": Anonymize<I2pjehun5ehh5i>;
}>;
export type I2pjehun5ehh5i = {
    "ident": Anonymize<Idkbvh6dahk1v7>;
};
export type I3mr3kek441eao = AnonymousEnum<{
    /**
     * Report voter equivocation/misbehavior. This method will verify the
     * equivocation proof and validate the given key ownership proof
     * against the extracted offender. If both are valid, the offence
     * will be reported.
     */
    "report_double_voting": Anonymize<I9nvbng83s6br2>;
    /**
     * Report voter equivocation/misbehavior. This method will verify the
     * equivocation proof and validate the given key ownership proof
     * against the extracted offender. If both are valid, the offence
     * will be reported.
     *
     * This extrinsic must be called unsigned and it is expected that only
     * block authors will call it (validated in `ValidateUnsigned`), as such
     * if the block author is defined it will be defined as the equivocation
     * reporter.
     */
    "report_double_voting_unsigned": Anonymize<I9nvbng83s6br2>;
    /**
     * Reset BEEFY consensus by setting a new BEEFY genesis at `delay_in_blocks` blocks in the
     * future.
     *
     * Note: `delay_in_blocks` has to be at least 1.
     */
    "set_new_genesis": Anonymize<Iemqna2uucuei9>;
    /**
     * Report fork voting equivocation. This method will verify the equivocation proof
     * and validate the given key ownership proof against the extracted offender.
     * If both are valid, the offence will be reported.
     */
    "report_fork_voting": Anonymize<Ibhcht7ff9e575>;
    /**
     * Report fork voting equivocation. This method will verify the equivocation proof
     * and validate the given key ownership proof against the extracted offender.
     * If both are valid, the offence will be reported.
     *
     * This extrinsic must be called unsigned and it is expected that only
     * block authors will call it (validated in `ValidateUnsigned`), as such
     * if the block author is defined it will be defined as the equivocation
     * reporter.
     */
    "report_fork_voting_unsigned": Anonymize<Ibhcht7ff9e575>;
    /**
     * Report future block voting equivocation. This method will verify the equivocation proof
     * and validate the given key ownership proof against the extracted offender.
     * If both are valid, the offence will be reported.
     */
    "report_future_block_voting": Anonymize<Ibtvrus3gmn010>;
    /**
     * Report future block voting equivocation. This method will verify the equivocation proof
     * and validate the given key ownership proof against the extracted offender.
     * If both are valid, the offence will be reported.
     *
     * This extrinsic must be called unsigned and it is expected that only
     * block authors will call it (validated in `ValidateUnsigned`), as such
     * if the block author is defined it will be defined as the equivocation
     * reporter.
     */
    "report_future_block_voting_unsigned": Anonymize<Ibtvrus3gmn010>;
}>;
export type I9nvbng83s6br2 = {
    "equivocation_proof": Anonymize<Ifiofttj73fsk1>;
};
export type Ifiofttj73fsk1 = {
    "first": Anonymize<I3eao7ea0kppv8>;
    "second": Anonymize<I3eao7ea0kppv8>;
};
export type I3eao7ea0kppv8 = {
    "commitment": {
        "payload": Array<[FixedSizeBinary<2>, Binary]>;
        "block_number": number;
        "validator_set_id": bigint;
    };
    "id": FixedSizeBinary<33>;
    "signature": FixedSizeBinary<65>;
};
export type Iemqna2uucuei9 = {
    "delay_in_blocks": number;
};
export type Ibhcht7ff9e575 = {
    "equivocation_proof": {
        "vote": Anonymize<I3eao7ea0kppv8>;
        "ancestry_proof": {
            "prev_peaks": Anonymize<Ic5m5lp1oioo8r>;
            "prev_leaf_count": bigint;
            "leaf_count": bigint;
            "items": Array<[bigint, FixedSizeBinary<32>]>;
        };
        "header": Anonymize<Ic952bubvq4k7d>;
    };
};
export type Ic952bubvq4k7d = {
    "parent_hash": FixedSizeBinary<32>;
    "number": number;
    "state_root": FixedSizeBinary<32>;
    "extrinsics_root": FixedSizeBinary<32>;
    "digest": Anonymize<I4mddgoa69c0a2>;
};
export type Ibtvrus3gmn010 = {
    "equivocation_proof": Anonymize<I3eao7ea0kppv8>;
};
export type I2p2dpv6jn1e8k = AnonymousEnum<{
    /**
     * Inherent to register any changes in the state of the Governed Map on Cardano compared to the state currently stored in the pallet.
     */
    "register_changes": Anonymize<Ib8ul16km22fkf>;
    /**
     * Changes the address of the Governed Map validator used for observation.
     *
     * This extrinsic must be run either using `sudo` or some other chain governance mechanism.
     */
    "set_main_chain_scripts": Anonymize<I5aub141nk0cu6>;
}>;
export type Ib8ul16km22fkf = {
    "changes": Array<[Binary, Anonymize<Iabpgqcjikia83>]>;
};
export type I5aub141nk0cu6 = {
    "new_main_chain_script": Anonymize<I2ho2o2f1oad8v>;
};
export type Idacla3pi5jort = (Anonymize<I2t1vhi8pcujcb>) | undefined;
export type Ie9sr1iqcg3cgm = ResultPayload<undefined, string>;
export type I1mqgk2tmnn9i2 = (string) | undefined;
export type I6lr8sctk0bi4e = Array<string>;
export type Iaqet9jc3ihboe = {
    "header": Anonymize<Ic952bubvq4k7d>;
    "extrinsics": Anonymize<Itom7fk49o0c9>;
};
export type I2v50gu3s1aqk6 = AnonymousEnum<{
    "AllExtrinsics": undefined;
    "OnlyInherents": undefined;
}>;
export type I4mqaaf6ie66ve = ResultPayload<Binary, Anonymize<Iq0i58g095lvm>>;
export type Iq0i58g095lvm = AnonymousEnum<{
    "Deserialization": Anonymize<Ihhhb06ltk59c>;
    "Serialization": Anonymize<Ichp8s4hhgp7ug>;
    "Transaction": Anonymize<Ic4hvjmrliv95i>;
    "LedgerCacheError": undefined;
    "NoLedgerState": undefined;
    "LedgerStateScaleDecodingError": undefined;
    "ContractCallCostError": undefined;
}>;
export type Ia1pome47r2aq5 = ResultPayload<{
    "hash": FixedSizeBinary<32>;
    "operations": Array<Enum<{
        "Call": {
            "address": Binary;
            "entry_point": Binary;
        };
        "Deploy": Anonymize<Id7ndeh7rg6a8t>;
        "Maintain": Anonymize<Id7ndeh7rg6a8t>;
        "ClaimMint": {
            "value": bigint;
            "coin_type": FixedSizeBinary<32>;
        };
    }>>;
    "identifiers": Anonymize<Itom7fk49o0c9>;
    "has_fallible_coins": boolean;
    "has_guaranteed_coins": boolean;
}, Anonymize<Iq0i58g095lvm>>;
export type Iehtf0ht2ndj33 = ResultPayload<bigint, Anonymize<Iq0i58g095lvm>>;
export type Ieduh03298o0nh = ResultPayload<[bigint, bigint], Anonymize<Iq0i58g095lvm>>;
export type I4p5t2krb1gmvp = [number, FixedSizeBinary<32>];
export type I8k1nb3hdf41md = ResultPayload<Anonymize<Icju1e6umh03pe>, Anonymize<I5nrjkj9qumobs>>;
export type I5nrjkj9qumobs = AnonymousEnum<{
    "Invalid": Enum<{
        "Call": undefined;
        "Payment": undefined;
        "Future": undefined;
        "Stale": undefined;
        "BadProof": undefined;
        "AncientBirthBlock": undefined;
        "ExhaustsResources": undefined;
        "Custom": number;
        "BadMandatory": undefined;
        "MandatoryValidation": undefined;
        "BadSigner": undefined;
        "IndeterminateImplicit": undefined;
        "UnknownOrigin": undefined;
    }>;
    "Unknown": TransactionValidityUnknownTransaction;
}>;
export type TransactionValidityUnknownTransaction = Enum<{
    "CannotLookup": undefined;
    "NoUnsignedValidator": undefined;
    "Custom": number;
}>;
export declare const TransactionValidityUnknownTransaction: GetEnum<TransactionValidityUnknownTransaction>;
export type If7uv525tdvv7a = Array<[FixedSizeBinary<8>, Binary]>;
export type I2an1fs2eiebjp = {
    "okay": boolean;
    "fatal_error": boolean;
    "errors": Anonymize<If7uv525tdvv7a>;
};
export type TransactionValidityTransactionSource = Enum<{
    "InBlock": undefined;
    "Local": undefined;
    "External": undefined;
}>;
export declare const TransactionValidityTransactionSource: GetEnum<TransactionValidityTransactionSource>;
export type I9ask1o4tfvcvs = ResultPayload<{
    "priority": bigint;
    "requires": Anonymize<Itom7fk49o0c9>;
    "provides": Anonymize<Itom7fk49o0c9>;
    "longevity": bigint;
    "propagate": boolean;
}, Anonymize<I5nrjkj9qumobs>>;
export type Ifogo2hpqpe6b4 = ({
    "validators": Anonymize<I2fb54desdqd9n>;
    "id": bigint;
}) | undefined;
export type I25plekc1moieu = {
    "vote": Anonymize<I3eao7ea0kppv8>;
    "ancestry_proof": Binary;
    "header": Anonymize<Ic952bubvq4k7d>;
};
export type I7rj2bnb76oko1 = ResultPayload<FixedSizeBinary<32>, MmrPrimitivesError>;
export type MmrPrimitivesError = Enum<{
    "InvalidNumericOp": undefined;
    "Push": undefined;
    "GetRoot": undefined;
    "Commit": undefined;
    "GenerateProof": undefined;
    "Verify": undefined;
    "LeafNotFound": undefined;
    "PalletNotIncluded": undefined;
    "InvalidLeafIndex": undefined;
    "InvalidBestKnownBlock": undefined;
}>;
export declare const MmrPrimitivesError: GetEnum<MmrPrimitivesError>;
export type I4o356o7eq06ms = ResultPayload<bigint, MmrPrimitivesError>;
export type I46e127tr8ma2h = ResultPayload<[Anonymize<Itom7fk49o0c9>, Anonymize<I38ee9is0n4jn9>], MmrPrimitivesError>;
export type I38ee9is0n4jn9 = {
    "leaf_indices": Array<bigint>;
    "leaf_count": bigint;
    "items": Anonymize<Ic5m5lp1oioo8r>;
};
export type Ie88mmnuvmuvp5 = ResultPayload<undefined, MmrPrimitivesError>;
export type Icerf8h8pdu8ss = (Array<[Binary, FixedSizeBinary<4>]>) | undefined;
export type I3944ctpg4imgb = {
    "epoch": bigint;
    "slot": bigint;
    "slots_per_epoch": number;
};
export type Ifpjvh1481i0rn = {
    "slots_per_epoch": number;
    "slot_duration": bigint;
};
export type I9v2ts6ja6a6bo = [bigint, Anonymize<I7tbts2dcf9pg2>];
export type If9hgm7bos9akg = (Anonymize<I9v2ts6ja6a6bo>) | undefined;
export type Ifro4ep1isjk6f = {
    "d_parameter": {
        "num_permissioned_candidates": number;
        "num_registered_candidates": number;
    };
    "permissioned_candidates": Array<Anonymize<Ieuflftcnd1rfe>>;
    "registered_candidates": Array<{
        "stake_pool_public_key": FixedSizeBinary<32>;
        "registrations": Array<Anonymize<I97al0h8mriqc3>>;
        "stake_delegation"?: Anonymize<I35p85j063s0il>;
    }>;
    "epoch_nonce": Binary;
};
export type Ieuflftcnd1rfe = {
    "sidechain_public_key": Binary;
    "aura_public_key": Binary;
    "grandpa_public_key": Binary;
};
export type I97al0h8mriqc3 = {
    "registration_utxo": Anonymize<Ib7m93p5rn57dr>;
    "sidechain_signature": Binary;
    "mainchain_signature": FixedSizeBinary<64>;
    "cross_chain_signature": Binary;
    "sidechain_pub_key": Binary;
    "cross_chain_pub_key": Binary;
    "utxo_info": {
        "utxo_id": Anonymize<Ib7m93p5rn57dr>;
        "epoch_number": number;
        "block_number": number;
        "slot_number": bigint;
        "tx_index_within_block": number;
    };
    "tx_inputs": Array<Anonymize<Ib7m93p5rn57dr>>;
    "aura_pub_key": Binary;
    "grandpa_pub_key": Binary;
};
export type I35p85j063s0il = (bigint) | undefined;
export type I7ihn3rkfc06gt = (Anonymize<I7tbts2dcf9pg2>) | undefined;
export type I6l5pmc2hu9ria = (Enum<{
    "InvalidMainchainSignature": undefined;
    "InvalidSidechainSignature": undefined;
    "InvalidTxInput": undefined;
    "InvalidMainchainPubKey": undefined;
    "InvalidSidechainPubKey": undefined;
    "InvalidAuraKey": undefined;
    "InvalidGrandpaKey": undefined;
}>) | undefined;
export type I6k6hm7lt3bcta = (Enum<{
    "InvalidStake": undefined;
    "UnknownStake": undefined;
}>) | undefined;
export type Inptormg87c56 = (Enum<{
    "InvalidSidechainPubKey": undefined;
    "InvalidAuraKey": undefined;
    "InvalidGrandpaKey": undefined;
}>) | undefined;
export type I2oedcvdsqsu3a = Array<[string, Binary]>;
export type Ibcsg1mcigvts3 = (Anonymize<I2ho2o2f1oad8v>) | undefined;
export type I43h535ocl8blv = AnonymousEnum<{
    "System": Anonymize<Iekve0i6djpd9f>;
    "Timestamp": Anonymize<I7d75gqfg6jh9c>;
    "Grandpa": Anonymize<Ibck9ekr2i96uj>;
    "Midnight": Anonymize<I912mlc35loovi>;
    "Balances": Anonymize<I9svldsp29mh87>;
    "Sudo": Anonymize<Ido5lcb08rfkrb>;
    "SessionCommitteeManagement": Anonymize<Ieu3guq2n6qufa>;
    "RuntimeUpgrade": Anonymize<I290u232f4cfcs>;
    "NativeTokenManagement": Anonymize<I2c42gmjg8ueu0>;
    "NativeTokenObservation": Anonymize<I7v9ukkd1fqtc7>;
    "Preimage": Anonymize<If81ks88t5mpk5>;
    "MultiBlockMigrations": Anonymize<I4oqb168b2d4er>;
    "PalletSession": Anonymize<I3vi1m70dcsram>;
    "Scheduler": Anonymize<I6hbqiqupggq6o>;
    "TxPause": Anonymize<Ieci88jft3cpv9>;
    "Beefy": Anonymize<I3mr3kek441eao>;
    "GovernedMap": Anonymize<I2p2dpv6jn1e8k>;
}>;
export {};
