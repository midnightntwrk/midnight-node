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

import { StorageDescriptor, PlainDescriptor, TxDescriptor, RuntimeDescriptor, Enum, ApisFromDef, QueryFromPalletsDef, TxFromPalletsDef, EventsFromPalletsDef, ErrorsFromPalletsDef, ConstFromPalletsDef, ViewFnsFromPalletsDef, SS58String, FixedSizeBinary, Binary, FixedSizeArray } from "polkadot-api";
import { I5sesotjlssv2d, Iffmde3ekjedi9, I4mddgoa69c0a2, I57rgindhd6rdr, I95g6i7ilua7lq, Ieniouoqkq4icf, Phase, Ibgl04rn6nbfm6, I4q39t5hn830vp, Ic5m5lp1oioo8r, GrandpaStoredState, I7pe2me3i3vtn9, I9jd27rnpm8ttv, I3geksg000c171, Ib7m93p5rn57dr, I1q8tnt1cluu5j, I8ds64oj6581v0, Ia7pdug7cdsg8g, I9bin2jc70qt6q, I9dgeh47ldhst1, I4r5bhov0m7jqr, I85ci61lv50332, I2t1vhi8pcujcb, I1am2l3mod97ls, I6l15kir0trh70, Idkbvh6dahk1v7, PreimageOldRequestStatus, I8j24837rs9r0t, I4pact7n2e9a0i, Iepbsvlk3qceij, Ia2lhg7l2hilo3, Ibslbpu3d5lodd, I2p9qi8l69c2sq, I82jm9g7pufuel, If7hhl1u9dung8, I56u24ncejr5kt, I2fb54desdqd9n, I4arjljr6dpflb, Idjett00s2gd, Icgljjb6j82uhn, I2ho2o2f1oad8v, In7a38730s6qs, If15el53dd76v9, I9s0ave7t0vnrk, I4fo08joqmcqnm, I8ofcg5rbj0g2c, I4adgbll7gku4i, I6pjjpfvhvcfru, I9pj91mj79qekl, I39uah9nss64h9, Ik64dknsq7k08, Ib51vk42m1po4n, Idcr6u6361oad9, I3a5kuu5t5jj3g, I2hviml3snvhhn, Ifsi09mj2o3peu, Iikug8jkjivr, Ib6fc72n7a066a, I5iilu2ehqsma0, I4ktuaksf5i1gk, I9bqtpv2ii35mp, I9j7pagd6d4bda, I2h9pmio37r7fb, Ibmr18suc9ikh9, I9iq22t0burs89, I5u8olqbbvfnvf, I5utcetro501ir, I6vutkc419j8di, I4qt71sd8urp33, I8k3rnvpeeh4hv, Idqpf8qfqdr537, Iaer7i07e9es8i, I2hrprd0b1n00, I9lbuvnpos4dqh, I27f772v76dngh, Id7ndeh7rg6a8t, I82nfqfkd48n10, I1jm8m1rh9e20v, I3o5j3bli1pd8e, Ibou4u1engb441, Id6nbvqoqdj4o2, I95iqep3b8snn9, Idk46ma354sc05, If1jstp94m1o1a, I5n4sebgkfr760, I31rh8oe57fm4v, Ifs1i5fk9cqvr6, Ic4ko9p2tkh2jm, I2tkgcau710d91, Ieg3fd8p4pkt10, I8kg5ll427kfqq, I467333262q1l9, Iba7pefg0d11kh, I2pjehun5ehh5i, I9nvbng83s6br2, Iemqna2uucuei9, Ibhcht7ff9e575, Ibtvrus3gmn010, Ib8ul16km22fkf, I5aub141nk0cu6, Ia82mnkmeo2rhc, I870h7pfmbjt5m, Icbccs0ug47ilf, I855j4i3kr8ko1, I7f4oosp2hcgjo, I5768ac424h061, I5o0in87i4h9qh, I7nfq6ftas0rri, I9qdvp794ab9dj, I3seth7anm0bu2, Icv68aq8841478, Ic262ibdoec56a, Iflcfm9b6nlmdd, Ijrsf4mnp3eka, Id5fm4p8lj5qgi, I8tjvj9uq4b7hi, I4cbvqmqadhrea, I3qt1hgg4djhgb, I4fooe9dun9o0t, I4npgd22g8nrk7, I5rtkmhm2dng4u, I3f33lun6ld0ef, I70pruthef0ilh, I6tccms8877uoj, I1ohl1lmd4r5j8, I7dt739idcq9qk, Ic1blifjtonodb, I4nibra9bv0ahp, I9i3o8bp584ej, If1co0pilmi7oq, I666bl2fqjkejo, Iae74gjak1qibn, I3escdojpj0551, I2hq50pu2kdjpo, I9acqruh7322g2, Ifhslud4mm5dob, Ia3c82eadg79bj, Ienusoeb625ftq, Ibtsa3docbr9el, Ihhhb06ltk59c, Ichp8s4hhgp7ug, Ic4hvjmrliv95i, Idacla3pi5jort, Ie9sr1iqcg3cgm, I1mqgk2tmnn9i2, Iabpgqcjikia83, I6lr8sctk0bi4e, Iaqet9jc3ihboe, Ic952bubvq4k7d, I2v50gu3s1aqk6, I4mqaaf6ie66ve, Ia1pome47r2aq5, Iehtf0ht2ndj33, Ieduh03298o0nh, I4p5t2krb1gmvp, I8k1nb3hdf41md, If7uv525tdvv7a, Itom7fk49o0c9, I2an1fs2eiebjp, TransactionValidityTransactionSource, I9ask1o4tfvcvs, Ifogo2hpqpe6b4, Ifiofttj73fsk1, I25plekc1moieu, I3eao7ea0kppv8, I7rj2bnb76oko1, I4o356o7eq06ms, I46e127tr8ma2h, I38ee9is0n4jn9, Ie88mmnuvmuvp5, Icerf8h8pdu8ss, I9puqgoda8ofk4, I3944ctpg4imgb, Ifpjvh1481i0rn, I9v2ts6ja6a6bo, If9hgm7bos9akg, Ifro4ep1isjk6f, I7ihn3rkfc06gt, I97al0h8mriqc3, I6l5pmc2hu9ria, I35p85j063s0il, I6k6hm7lt3bcta, Ieuflftcnd1rfe, Inptormg87c56, I2oedcvdsqsu3a, Ibcsg1mcigvts3, I43h535ocl8blv, Im2qle9pka0f8 } from "./common-types";
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
type IStorage = {
    System: {
        /**
         * The full account information for a particular account ID.
         */
        Account: StorageDescriptor<[Key: SS58String], Anonymize<I5sesotjlssv2d>, false, never>;
        /**
         * Total extrinsics count for the current block.
         */
        ExtrinsicCount: StorageDescriptor<[], number, true, never>;
        /**
         * Whether all inherents have been applied.
         */
        InherentsApplied: StorageDescriptor<[], boolean, false, never>;
        /**
         * The current weight for the block.
         */
        BlockWeight: StorageDescriptor<[], Anonymize<Iffmde3ekjedi9>, false, never>;
        /**
         * Total length (in bytes) for all extrinsics put together, for the current block.
         */
        AllExtrinsicsLen: StorageDescriptor<[], number, true, never>;
        /**
         * Map of block numbers to block hashes.
         */
        BlockHash: StorageDescriptor<[Key: number], FixedSizeBinary<32>, false, never>;
        /**
         * Extrinsics data for the current block (maps an extrinsic's index to its data).
         */
        ExtrinsicData: StorageDescriptor<[Key: number], Binary, false, never>;
        /**
         * The current block number being processed. Set by `execute_block`.
         */
        Number: StorageDescriptor<[], number, false, never>;
        /**
         * Hash of the previous block.
         */
        ParentHash: StorageDescriptor<[], FixedSizeBinary<32>, false, never>;
        /**
         * Digest of the current block, also part of the block header.
         */
        Digest: StorageDescriptor<[], Anonymize<I4mddgoa69c0a2>, false, never>;
        /**
         * Events deposited for the current block.
         *
         * NOTE: The item is unbound and should therefore never be read on chain.
         * It could otherwise inflate the PoV size of a block.
         *
         * Events have a large in-memory size. Box the events to not go out-of-memory
         * just in case someone still reads them from within the runtime.
         */
        Events: StorageDescriptor<[], Anonymize<I57rgindhd6rdr>, false, never>;
        /**
         * The number of events in the `Events<T>` list.
         */
        EventCount: StorageDescriptor<[], number, false, never>;
        /**
         * Mapping between a topic (represented by T::Hash) and a vector of indexes
         * of events in the `<Events<T>>` list.
         *
         * All topic vectors have deterministic storage locations depending on the topic. This
         * allows light-clients to leverage the changes trie storage tracking mechanism and
         * in case of changes fetch the list of events of interest.
         *
         * The value has the type `(BlockNumberFor<T>, EventIndex)` because if we used only just
         * the `EventIndex` then in case if the topic has the same contents on the next block
         * no notification will be triggered thus the event might be lost.
         */
        EventTopics: StorageDescriptor<[Key: FixedSizeBinary<32>], Anonymize<I95g6i7ilua7lq>, false, never>;
        /**
         * Stores the `spec_version` and `spec_name` of when the last runtime upgrade happened.
         */
        LastRuntimeUpgrade: StorageDescriptor<[], Anonymize<Ieniouoqkq4icf>, true, never>;
        /**
         * True if we have upgraded so that `type RefCount` is `u32`. False (default) if not.
         */
        UpgradedToU32RefCount: StorageDescriptor<[], boolean, false, never>;
        /**
         * True if we have upgraded so that AccountInfo contains three types of `RefCount`. False
         * (default) if not.
         */
        UpgradedToTripleRefCount: StorageDescriptor<[], boolean, false, never>;
        /**
         * The execution phase of the block.
         */
        ExecutionPhase: StorageDescriptor<[], Phase, true, never>;
        /**
         * `Some` if a code upgrade has been authorized.
         */
        AuthorizedUpgrade: StorageDescriptor<[], Anonymize<Ibgl04rn6nbfm6>, true, never>;
        /**
         * The weight reclaimed for the extrinsic.
         *
         * This information is available until the end of the extrinsic execution.
         * More precisely this information is removed in `note_applied_extrinsic`.
         *
         * Logic doing some post dispatch weight reduction must update this storage to avoid duplicate
         * reduction.
         */
        ExtrinsicWeightReclaimed: StorageDescriptor<[], Anonymize<I4q39t5hn830vp>, false, never>;
    };
    Timestamp: {
        /**
         * The current time for the current block.
         */
        Now: StorageDescriptor<[], bigint, false, never>;
        /**
         * Whether the timestamp has been updated in this block.
         *
         * This value is updated to `true` upon successful submission of a timestamp by a node.
         * It is then checked at the end of each block execution in the `on_finalize` hook.
         */
        DidUpdate: StorageDescriptor<[], boolean, false, never>;
    };
    Aura: {
        /**
         * The current authority set.
         */
        Authorities: StorageDescriptor<[], Anonymize<Ic5m5lp1oioo8r>, false, never>;
        /**
         * The current slot of this block.
         *
         * This will be set in `on_initialize`.
         */
        CurrentSlot: StorageDescriptor<[], bigint, false, never>;
    };
    Grandpa: {
        /**
         * State of the current authority set.
         */
        State: StorageDescriptor<[], GrandpaStoredState, false, never>;
        /**
         * Pending change: (signaled at, scheduled change).
         */
        PendingChange: StorageDescriptor<[], Anonymize<I7pe2me3i3vtn9>, true, never>;
        /**
         * next block number where we can force a change.
         */
        NextForced: StorageDescriptor<[], number, true, never>;
        /**
         * `true` if we are currently stalled.
         */
        Stalled: StorageDescriptor<[], Anonymize<I9jd27rnpm8ttv>, true, never>;
        /**
         * The number of changes (both in terms of keys and underlying economic responsibilities)
         * in the "set" of Grandpa validators from genesis.
         */
        CurrentSetId: StorageDescriptor<[], bigint, false, never>;
        /**
         * A mapping from grandpa set ID to the index of the *most recent* session for which its
         * members were responsible.
         *
         * This is only used for validating equivocation proofs. An equivocation proof must
         * contains a key-ownership proof for a given session, therefore we need a way to tie
         * together sessions and GRANDPA set ids, i.e. we need to validate that a validator
         * was the owner of a given key on a given session, and what the active set ID was
         * during that session.
         *
         * TWOX-NOTE: `SetId` is not under user control.
         */
        SetIdSession: StorageDescriptor<[Key: bigint], number, true, never>;
        /**
         * The current list of authorities.
         */
        Authorities: StorageDescriptor<[], Anonymize<I3geksg000c171>, false, never>;
    };
    Sidechain: {
        /**
         * Current epoch number
         */
        EpochNumber: StorageDescriptor<[], bigint, false, never>;
        /**
         * Number of slots per epoch. Currently this value must not change for a running chain.
         */
        SlotsPerEpoch: StorageDescriptor<[], number, false, never>;
        /**
         * Genesis Cardano UTXO of the Partner Chain
         *
         * This is the UTXO that is burned by the transaction that establishes Partner Chain
         * governance on Cardano and serves as the identifier of the Partner Chain. It is also
         * included in various signed messages to prevent replay attacks on other Partner Chains.
         */
        GenesisUtxo: StorageDescriptor<[], Anonymize<Ib7m93p5rn57dr>, false, never>;
    };
    Midnight: {
        /**
        
         */
        StateKey: StorageDescriptor<[], Binary, true, never>;
        /**
        
         */
        NetworkId: StorageDescriptor<[], Binary, true, never>;
        /**
        
         */
        DParameterOverride: StorageDescriptor<[], Anonymize<I9jd27rnpm8ttv>, true, never>;
        /**
        
         */
        ConfigurableWeight: StorageDescriptor<[], Anonymize<I4q39t5hn830vp>, false, never>;
        /**
        
         */
        ConfigurableContractCallWeight: StorageDescriptor<[], Anonymize<I4q39t5hn830vp>, false, never>;
        /**
        
         */
        ConfigurableTransactionSizeWeight: StorageDescriptor<[], Anonymize<I4q39t5hn830vp>, false, never>;
        /**
        
         */
        SafeMode: StorageDescriptor<[], boolean, false, never>;
    };
    Balances: {
        /**
         * The total units issued in the system.
         */
        TotalIssuance: StorageDescriptor<[], bigint, false, never>;
        /**
         * The total units of outstanding deactivated balance in the system.
         */
        InactiveIssuance: StorageDescriptor<[], bigint, false, never>;
        /**
         * The Balances pallet example of storing the balance of an account.
         *
         * # Example
         *
         * ```nocompile
         * impl pallet_balances::Config for Runtime {
         * type AccountStore = StorageMapShim<Self::Account<Runtime>, frame_system::Provider<Runtime>, AccountId, Self::AccountData<Balance>>
         * }
         * ```
         *
         * You can also store the balance of an account in the `System` pallet.
         *
         * # Example
         *
         * ```nocompile
         * impl pallet_balances::Config for Runtime {
         * type AccountStore = System
         * }
         * ```
         *
         * But this comes with tradeoffs, storing account balances in the system pallet stores
         * `frame_system` data alongside the account data contrary to storing account balances in the
         * `Balances` pallet, which uses a `StorageMap` to store balances data only.
         * NOTE: This is only used in the case that this pallet is used to store balances.
         */
        Account: StorageDescriptor<[Key: SS58String], Anonymize<I1q8tnt1cluu5j>, false, never>;
        /**
         * Any liquidity locks on some account balances.
         * NOTE: Should only be accessed when setting, changing and freeing a lock.
         *
         * Use of locks is deprecated in favour of freezes. See `https://github.com/paritytech/substrate/pull/12951/`
         */
        Locks: StorageDescriptor<[Key: SS58String], Anonymize<I8ds64oj6581v0>, false, never>;
        /**
         * Named reserves on some account balances.
         *
         * Use of reserves is deprecated in favour of holds. See `https://github.com/paritytech/substrate/pull/12951/`
         */
        Reserves: StorageDescriptor<[Key: SS58String], Anonymize<Ia7pdug7cdsg8g>, false, never>;
        /**
         * Holds on account balances.
         */
        Holds: StorageDescriptor<[Key: SS58String], Anonymize<I9bin2jc70qt6q>, false, never>;
        /**
         * Freeze locks on account balances.
         */
        Freezes: StorageDescriptor<[Key: SS58String], Anonymize<I9bin2jc70qt6q>, false, never>;
    };
    Sudo: {
        /**
         * The `AccountId` of the sudo key.
         */
        Key: StorageDescriptor<[], SS58String, true, never>;
    };
    SessionCommitteeManagement: {
        /**
        
         */
        CurrentCommittee: StorageDescriptor<[], Anonymize<I9dgeh47ldhst1>, false, never>;
        /**
        
         */
        NextCommittee: StorageDescriptor<[], Anonymize<I9dgeh47ldhst1>, true, never>;
        /**
        
         */
        MainChainScriptsConfiguration: StorageDescriptor<[], Anonymize<I4r5bhov0m7jqr>, false, never>;
    };
    RuntimeUpgrade: {
        /**
        
         */
        RuntimeUpgradeVotes: StorageDescriptor<[], Anonymize<I85ci61lv50332>, false, never>;
    };
    NativeTokenManagement: {
        /**
        
         */
        MainChainScriptsConfiguration: StorageDescriptor<[], Anonymize<I2t1vhi8pcujcb>, true, never>;
        /**
         * Stores the pallet's initialization state.
         *
         * The pallet is considered initialized if its inherent has been successfuly called at least once since
         * genesis or the last invocation of [set_main_chain_scripts][Pallet::set_main_chain_scripts].
         */
        Initialized: StorageDescriptor<[], boolean, false, never>;
        /**
         * Transient storage containing the amount of native token transfer registered in the current block.
         *
         * Any value in this storage is only present during execution of a block and is emptied on block finalization.
         */
        TransferedThisBlock: StorageDescriptor<[], bigint, true, never>;
    };
    NativeTokenObservation: {
        /**
        
         */
        MainChainGenerationRegistrantsAddress: StorageDescriptor<[], Binary, false, never>;
        /**
        
         */
        Registrations: StorageDescriptor<[Key: Binary], Anonymize<I1am2l3mod97ls>, false, never>;
        /**
        
         */
        UtxoOwners: StorageDescriptor<[Key: Binary], Binary, true, never>;
        /**
        
         */
        NextCardanoPosition: StorageDescriptor<[], Anonymize<I6l15kir0trh70>, false, never>;
        /**
        
         */
        NativeAssetIdentifier: StorageDescriptor<[], Anonymize<Idkbvh6dahk1v7>, false, never>;
        /**
        
         */
        CardanoBlockWindowSize: StorageDescriptor<[], number, false, never>;
        /**
         * Max amount of Cardano transactions that can be processed per block
         */
        CardanoTxCapacityPerBlock: StorageDescriptor<[], number, false, never>;
    };
    Preimage: {
        /**
         * The request status of a given hash.
         */
        StatusFor: StorageDescriptor<[Key: FixedSizeBinary<32>], PreimageOldRequestStatus, true, never>;
        /**
         * The request status of a given hash.
         */
        RequestStatusFor: StorageDescriptor<[Key: FixedSizeBinary<32>], Anonymize<I8j24837rs9r0t>, true, never>;
        /**
        
         */
        PreimageFor: StorageDescriptor<[Key: Anonymize<I4pact7n2e9a0i>], Binary, true, never>;
    };
    MultiBlockMigrations: {
        /**
         * The currently active migration to run and its cursor.
         *
         * `None` indicates that no migration is running.
         */
        Cursor: StorageDescriptor<[], Anonymize<Iepbsvlk3qceij>, true, never>;
        /**
         * Set of all successfully executed migrations.
         *
         * This is used as blacklist, to not re-execute migrations that have not been removed from the
         * codebase yet. Governance can regularly clear this out via `clear_historic`.
         */
        Historic: StorageDescriptor<[Key: Binary], null, true, never>;
    };
    PalletSession: {
        /**
         * The current set of validators.
         */
        Validators: StorageDescriptor<[], Anonymize<Ia2lhg7l2hilo3>, false, never>;
        /**
         * Current index of the session.
         */
        CurrentIndex: StorageDescriptor<[], number, false, never>;
        /**
         * True if the underlying economic identities or weighting behind the validators
         * has changed in the queued validator set.
         */
        QueuedChanged: StorageDescriptor<[], boolean, false, never>;
        /**
         * The queued keys for the next session. When the next session begins, these keys
         * will be used to determine the validator's session keys.
         */
        QueuedKeys: StorageDescriptor<[], Anonymize<Ibslbpu3d5lodd>, false, never>;
        /**
         * Indices of disabled validators.
         *
         * The vec is always kept sorted so that we can find whether a given validator is
         * disabled using binary search. It gets cleared when `on_session_ending` returns
         * a new set of identities.
         */
        DisabledValidators: StorageDescriptor<[], Anonymize<I95g6i7ilua7lq>, false, never>;
        /**
         * The next session keys for a validator.
         */
        NextKeys: StorageDescriptor<[Key: SS58String], Anonymize<I2p9qi8l69c2sq>, true, never>;
        /**
         * The owner of a key. The key is the `KeyTypeId` + the encoded key.
         */
        KeyOwner: StorageDescriptor<[Key: Anonymize<I82jm9g7pufuel>], SS58String, true, never>;
    };
    Scheduler: {
        /**
         * Block number at which the agenda began incomplete execution.
         */
        IncompleteSince: StorageDescriptor<[], number, true, never>;
        /**
         * Items to be executed, indexed by the block number that they should be executed on.
         */
        Agenda: StorageDescriptor<[Key: number], Anonymize<If7hhl1u9dung8>, false, never>;
        /**
         * Retry configurations for items to be executed, indexed by task address.
         */
        Retries: StorageDescriptor<[Key: Anonymize<I9jd27rnpm8ttv>], Anonymize<I56u24ncejr5kt>, true, never>;
        /**
         * Lookup from a name to the block number and index of the task.
         *
         * For v3 -> v4 the previously unbounded identities are Blake2-256 hashed to form the v4
         * identities.
         */
        Lookup: StorageDescriptor<[Key: FixedSizeBinary<32>], Anonymize<I9jd27rnpm8ttv>, true, never>;
    };
    TxPause: {
        /**
         * The set of calls that are explicitly paused.
         */
        PausedCalls: StorageDescriptor<[Key: Anonymize<Idkbvh6dahk1v7>], null, true, never>;
    };
    Beefy: {
        /**
         * The current authorities set
         */
        Authorities: StorageDescriptor<[], Anonymize<I2fb54desdqd9n>, false, never>;
        /**
         * The current validator set id
         */
        ValidatorSetId: StorageDescriptor<[], bigint, false, never>;
        /**
         * Authorities set scheduled to be used with the next session
         */
        NextAuthorities: StorageDescriptor<[], Anonymize<I2fb54desdqd9n>, false, never>;
        /**
         * A mapping from BEEFY set ID to the index of the *most recent* session for which its
         * members were responsible.
         *
         * This is only used for validating equivocation proofs. An equivocation proof must
         * contains a key-ownership proof for a given session, therefore we need a way to tie
         * together sessions and BEEFY set ids, i.e. we need to validate that a validator
         * was the owner of a given key on a given session, and what the active set ID was
         * during that session.
         *
         * TWOX-NOTE: `ValidatorSetId` is not under user control.
         */
        SetIdSession: StorageDescriptor<[Key: bigint], number, true, never>;
        /**
         * Block number where BEEFY consensus is enabled/started.
         * By changing this (through privileged `set_new_genesis()`), BEEFY consensus is effectively
         * restarted from the newly set block number.
         */
        GenesisBlock: StorageDescriptor<[], Anonymize<I4arjljr6dpflb>, false, never>;
    };
    Mmr: {
        /**
         * Latest MMR Root hash.
         */
        RootHash: StorageDescriptor<[], FixedSizeBinary<32>, false, never>;
        /**
         * Current size of the MMR (number of leaves).
         */
        NumberOfLeaves: StorageDescriptor<[], bigint, false, never>;
        /**
         * Hashes of the nodes in the MMR.
         *
         * Note this collection only contains MMR peaks, the inner nodes (and leaves)
         * are pruned and only stored in the Offchain DB.
         */
        Nodes: StorageDescriptor<[Key: bigint], FixedSizeBinary<32>, true, never>;
    };
    BeefyMmrLeaf: {
        /**
         * Details of current BEEFY authority set.
         */
        BeefyAuthorities: StorageDescriptor<[], Anonymize<Idjett00s2gd>, false, never>;
        /**
         * Details of next BEEFY authority set.
         *
         * This storage entry is used as cache for calls to `update_beefy_next_authority_set`.
         */
        BeefyNextAuthorities: StorageDescriptor<[], Anonymize<Idjett00s2gd>, false, never>;
    };
    Session: {
        /**
        
         */
        Validators: StorageDescriptor<[], Anonymize<Ia2lhg7l2hilo3>, false, never>;
        /**
        
         */
        ValidatorsAndKeys: StorageDescriptor<[], Anonymize<Ibslbpu3d5lodd>, false, never>;
        /**
         * Current index of the session.
         */
        CurrentIndex: StorageDescriptor<[], number, false, never>;
        /**
         * Indices of disabled validators.
         *
         * The vec is always kept sorted so that we can find whether a given validator is
         * disabled using binary search. It gets cleared when `on_session_ending` returns
         * a new set of identities.
         */
        DisabledValidators: StorageDescriptor<[], Anonymize<Icgljjb6j82uhn>, false, never>;
    };
    GovernedMap: {
        /**
         * Stores the initialization state of the pallet
         *
         * The pallet is considered uninitialized if no inherent was executed since the genesis block or
         * since the last change of the main chain scripts.
         */
        Initialized: StorageDescriptor<[], boolean, false, never>;
        /**
         * Stores the block number of the last time mapping changes were registered
         */
        LastUpdateBlock: StorageDescriptor<[], number, true, never>;
        /**
         * Stores the latest state of the Governed Map that was observed on Cardano.
         */
        Mapping: StorageDescriptor<[Key: Binary], Binary, true, never>;
        /**
         * Cardano address of the Governed Map validator.
         *
         * This address is used by the observability component to query current state of the mapping
         */
        MainChainScripts: StorageDescriptor<[], Anonymize<I2ho2o2f1oad8v>, true, never>;
    };
};
type ICalls = {
    System: {
        /**
         * Make some on-chain remark.
         *
         * Can be executed by every `origin`.
         */
        remark: TxDescriptor<Anonymize<I8ofcg5rbj0g2c>>;
        /**
         * Set the number of pages in the WebAssembly environment's heap.
         */
        set_heap_pages: TxDescriptor<Anonymize<I4adgbll7gku4i>>;
        /**
         * Set the new runtime code.
         */
        set_code: TxDescriptor<Anonymize<I6pjjpfvhvcfru>>;
        /**
         * Set the new runtime code without doing any checks of the given `code`.
         *
         * Note that runtime upgrades will not run if this is called with a not-increasing spec
         * version!
         */
        set_code_without_checks: TxDescriptor<Anonymize<I6pjjpfvhvcfru>>;
        /**
         * Set some items of storage.
         */
        set_storage: TxDescriptor<Anonymize<I9pj91mj79qekl>>;
        /**
         * Kill some items from storage.
         */
        kill_storage: TxDescriptor<Anonymize<I39uah9nss64h9>>;
        /**
         * Kill all storage items with a key that starts with the given prefix.
         *
         * **NOTE:** We rely on the Root origin to provide us the number of subkeys under
         * the prefix we are removing to accurately calculate the weight of this function.
         */
        kill_prefix: TxDescriptor<Anonymize<Ik64dknsq7k08>>;
        /**
         * Make some on-chain remark and emit event.
         */
        remark_with_event: TxDescriptor<Anonymize<I8ofcg5rbj0g2c>>;
        /**
         * Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied
         * later.
         *
         * This call requires Root origin.
         */
        authorize_upgrade: TxDescriptor<Anonymize<Ib51vk42m1po4n>>;
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
        authorize_upgrade_without_checks: TxDescriptor<Anonymize<Ib51vk42m1po4n>>;
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
        apply_authorized_upgrade: TxDescriptor<Anonymize<I6pjjpfvhvcfru>>;
    };
    Timestamp: {
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
        set: TxDescriptor<Anonymize<Idcr6u6361oad9>>;
    };
    Grandpa: {
        /**
         * Report voter equivocation/misbehavior. This method will verify the
         * equivocation proof and validate the given key ownership proof
         * against the extracted offender. If both are valid, the offence
         * will be reported.
         */
        report_equivocation: TxDescriptor<Anonymize<I3a5kuu5t5jj3g>>;
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
        report_equivocation_unsigned: TxDescriptor<Anonymize<I3a5kuu5t5jj3g>>;
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
        note_stalled: TxDescriptor<Anonymize<I2hviml3snvhhn>>;
    };
    Midnight: {
        /**
        
         */
        send_mn_transaction: TxDescriptor<Anonymize<Ifsi09mj2o3peu>>;
        /**
        
         */
        set_mn_tx_weight: TxDescriptor<Anonymize<Iikug8jkjivr>>;
        /**
        
         */
        override_d_parameter: TxDescriptor<Anonymize<Ib6fc72n7a066a>>;
        /**
        
         */
        set_contract_call_weight: TxDescriptor<Anonymize<Iikug8jkjivr>>;
        /**
        
         */
        set_tx_size_weight: TxDescriptor<Anonymize<Iikug8jkjivr>>;
        /**
        
         */
        set_safe_mode: TxDescriptor<Anonymize<I5iilu2ehqsma0>>;
    };
    Balances: {
        /**
         * Transfer some liquid free balance to another account.
         *
         * `transfer_allow_death` will set the `FreeBalance` of the sender and receiver.
         * If the sender's account is below the existential deposit as a result
         * of the transfer, the account will be reaped.
         *
         * The dispatch origin for this call must be `Signed` by the transactor.
         */
        transfer_allow_death: TxDescriptor<Anonymize<I4ktuaksf5i1gk>>;
        /**
         * Exactly as `transfer_allow_death`, except the origin must be root and the source account
         * may be specified.
         */
        force_transfer: TxDescriptor<Anonymize<I9bqtpv2ii35mp>>;
        /**
         * Same as the [`transfer_allow_death`] call, but with a check that the transfer will not
         * kill the origin account.
         *
         * 99% of the time you want [`transfer_allow_death`] instead.
         *
         * [`transfer_allow_death`]: struct.Pallet.html#method.transfer
         */
        transfer_keep_alive: TxDescriptor<Anonymize<I4ktuaksf5i1gk>>;
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
        transfer_all: TxDescriptor<Anonymize<I9j7pagd6d4bda>>;
        /**
         * Unreserve some balance from a user by force.
         *
         * Can only be called by ROOT.
         */
        force_unreserve: TxDescriptor<Anonymize<I2h9pmio37r7fb>>;
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
        upgrade_accounts: TxDescriptor<Anonymize<Ibmr18suc9ikh9>>;
        /**
         * Set the regular balance of a given account.
         *
         * The dispatch origin for this call is `root`.
         */
        force_set_balance: TxDescriptor<Anonymize<I9iq22t0burs89>>;
        /**
         * Adjust the total issuance in a saturating way.
         *
         * Can only be called by root and always needs a positive `delta`.
         *
         * # Example
         */
        force_adjust_total_issuance: TxDescriptor<Anonymize<I5u8olqbbvfnvf>>;
        /**
         * Burn the specified liquid free balance from the origin account.
         *
         * If the origin's account ends up below the existential deposit as a result
         * of the burn and `keep_alive` is false, the account will be reaped.
         *
         * Unlike sending funds to a _burn_ address, which merely makes the funds inaccessible,
         * this `burn` operation will reduce total issuance by the amount _burned_.
         */
        burn: TxDescriptor<Anonymize<I5utcetro501ir>>;
    };
    Sudo: {
        /**
         * Authenticates the sudo key and dispatches a function call with `Root` origin.
         */
        sudo: TxDescriptor<Anonymize<I6vutkc419j8di>>;
        /**
         * Authenticates the sudo key and dispatches a function call with `Root` origin.
         * This function does not check the weight of the call, and instead allows the
         * Sudo user to specify the weight of the call.
         *
         * The dispatch origin for this call must be _Signed_.
         */
        sudo_unchecked_weight: TxDescriptor<Anonymize<I4qt71sd8urp33>>;
        /**
         * Authenticates the current sudo key and sets the given AccountId (`new`) as the new sudo
         * key.
         */
        set_key: TxDescriptor<Anonymize<I8k3rnvpeeh4hv>>;
        /**
         * Authenticates the sudo key and dispatches a function call with `Signed` origin from
         * a given account.
         *
         * The dispatch origin for this call must be _Signed_.
         */
        sudo_as: TxDescriptor<Anonymize<Idqpf8qfqdr537>>;
        /**
         * Permanently removes the sudo key.
         *
         * **This cannot be un-done.**
         */
        remove_key: TxDescriptor<undefined>;
    };
    SessionCommitteeManagement: {
        /**
         * 'for_epoch_number' parameter is needed only for validation purposes, because we need to make sure that
         * check_inherent uses the same epoch_number as was used to create inherent data.
         * Alternative approach would be to put epoch number inside InherentData. However, sidechain
         * epoch number is set in Runtime, thus, inherent data provider doesn't have to know about it.
         * On top of that, the latter approach is slightly more complicated to code.
         */
        set: TxDescriptor<Anonymize<Iaer7i07e9es8i>>;
        /**
         * Changes the main chain scripts used for committee rotation.
         *
         * This extrinsic must be run either using `sudo` or some other chain governance mechanism.
         */
        set_main_chain_scripts: TxDescriptor<Anonymize<I4r5bhov0m7jqr>>;
    };
    RuntimeUpgrade: {
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
        propose_or_vote_upgrade: TxDescriptor<Anonymize<I2hrprd0b1n00>>;
    };
    NativeTokenManagement: {
        /**
         * Inherent that registers new native token transfer from the Cardano main chain and triggers
         * the handler configured in [Config::TokenTransferHandler].
         *
         * Arguments:
         * - `token_amount`: the total amount of tokens transferred since the last invocation of the inherent
         */
        transfer_tokens: TxDescriptor<Anonymize<I9lbuvnpos4dqh>>;
        /**
         * Changes the main chain scripts used for observing native token transfers.
         *
         * This extrinsic must be run either using `sudo` or some other chain governance mechanism.
         */
        set_main_chain_scripts: TxDescriptor<Anonymize<I2t1vhi8pcujcb>>;
    };
    NativeTokenObservation: {
        /**
        
         */
        process_tokens: TxDescriptor<Anonymize<I27f772v76dngh>>;
        /**
         * Changes the mainchain address for the mapping validator contract
         *
         * This extrinsic must be run either using `sudo` or some other chain governance mechanism.
         */
        set_mapping_validator_contract_address: TxDescriptor<Anonymize<Id7ndeh7rg6a8t>>;
    };
    Preimage: {
        /**
         * Register a preimage on-chain.
         *
         * If the preimage was previously requested, no fees or deposits are taken for providing
         * the preimage. Otherwise, a deposit is taken proportional to the size of the preimage.
         */
        note_preimage: TxDescriptor<Anonymize<I82nfqfkd48n10>>;
        /**
         * Clear an unrequested preimage from the runtime storage.
         *
         * If `len` is provided, then it will be a much cheaper operation.
         *
         * - `hash`: The hash of the preimage to be removed from the store.
         * - `len`: The length of the preimage of `hash`.
         */
        unnote_preimage: TxDescriptor<Anonymize<I1jm8m1rh9e20v>>;
        /**
         * Request a preimage be uploaded to the chain without paying any fees or deposits.
         *
         * If the preimage requests has already been provided on-chain, we unreserve any deposit
         * a user may have paid, and take the control of the preimage out of their hands.
         */
        request_preimage: TxDescriptor<Anonymize<I1jm8m1rh9e20v>>;
        /**
         * Clear a previously made request for a preimage.
         *
         * NOTE: THIS MUST NOT BE CALLED ON `hash` MORE TIMES THAN `request_preimage`.
         */
        unrequest_preimage: TxDescriptor<Anonymize<I1jm8m1rh9e20v>>;
        /**
         * Ensure that the bulk of pre-images is upgraded.
         *
         * The caller pays no fee if at least 90% of pre-images were successfully updated.
         */
        ensure_updated: TxDescriptor<Anonymize<I3o5j3bli1pd8e>>;
    };
    MultiBlockMigrations: {
        /**
         * Allows root to set a cursor to forcefully start, stop or forward the migration process.
         *
         * Should normally not be needed and is only in place as emergency measure. Note that
         * restarting the migration process in this manner will not call the
         * [`MigrationStatusHandler::started`] hook or emit an `UpgradeStarted` event.
         */
        force_set_cursor: TxDescriptor<Anonymize<Ibou4u1engb441>>;
        /**
         * Allows root to set an active cursor to forcefully start/forward the migration process.
         *
         * This is an edge-case version of [`Self::force_set_cursor`] that allows to set the
         * `started_at` value to the next block number. Otherwise this would not be possible, since
         * `force_set_cursor` takes an absolute block number. Setting `started_at` to `None`
         * indicates that the current block number plus one should be used.
         */
        force_set_active_cursor: TxDescriptor<Anonymize<Id6nbvqoqdj4o2>>;
        /**
         * Forces the onboarding of the migrations.
         *
         * This process happens automatically on a runtime upgrade. It is in place as an emergency
         * measurement. The cursor needs to be `None` for this to succeed.
         */
        force_onboard_mbms: TxDescriptor<undefined>;
        /**
         * Clears the `Historic` set.
         *
         * `map_cursor` must be set to the last value that was returned by the
         * `HistoricCleared` event. The first time `None` can be used. `limit` must be chosen in a
         * way that will result in a sensible weight.
         */
        clear_historic: TxDescriptor<Anonymize<I95iqep3b8snn9>>;
    };
    PalletSession: {
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
        set_keys: TxDescriptor<Anonymize<Idk46ma354sc05>>;
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
        purge_keys: TxDescriptor<undefined>;
    };
    Scheduler: {
        /**
         * Anonymously schedule a task.
         */
        schedule: TxDescriptor<Anonymize<If1jstp94m1o1a>>;
        /**
         * Cancel an anonymously scheduled task.
         */
        cancel: TxDescriptor<Anonymize<I5n4sebgkfr760>>;
        /**
         * Schedule a named task.
         */
        schedule_named: TxDescriptor<Anonymize<I31rh8oe57fm4v>>;
        /**
         * Cancel a named scheduled task.
         */
        cancel_named: TxDescriptor<Anonymize<Ifs1i5fk9cqvr6>>;
        /**
         * Anonymously schedule a task after a delay.
         */
        schedule_after: TxDescriptor<Anonymize<Ic4ko9p2tkh2jm>>;
        /**
         * Schedule a named task after a delay.
         */
        schedule_named_after: TxDescriptor<Anonymize<I2tkgcau710d91>>;
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
        set_retry: TxDescriptor<Anonymize<Ieg3fd8p4pkt10>>;
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
        set_retry_named: TxDescriptor<Anonymize<I8kg5ll427kfqq>>;
        /**
         * Removes the retry configuration of a task.
         */
        cancel_retry: TxDescriptor<Anonymize<I467333262q1l9>>;
        /**
         * Cancel the retry configuration of a named task.
         */
        cancel_retry_named: TxDescriptor<Anonymize<Ifs1i5fk9cqvr6>>;
    };
    TxPause: {
        /**
         * Pause a call.
         *
         * Can only be called by [`Config::PauseOrigin`].
         * Emits an [`Event::CallPaused`] event on success.
         */
        pause: TxDescriptor<Anonymize<Iba7pefg0d11kh>>;
        /**
         * Un-pause a call.
         *
         * Can only be called by [`Config::UnpauseOrigin`].
         * Emits an [`Event::CallUnpaused`] event on success.
         */
        unpause: TxDescriptor<Anonymize<I2pjehun5ehh5i>>;
    };
    Beefy: {
        /**
         * Report voter equivocation/misbehavior. This method will verify the
         * equivocation proof and validate the given key ownership proof
         * against the extracted offender. If both are valid, the offence
         * will be reported.
         */
        report_double_voting: TxDescriptor<Anonymize<I9nvbng83s6br2>>;
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
        report_double_voting_unsigned: TxDescriptor<Anonymize<I9nvbng83s6br2>>;
        /**
         * Reset BEEFY consensus by setting a new BEEFY genesis at `delay_in_blocks` blocks in the
         * future.
         *
         * Note: `delay_in_blocks` has to be at least 1.
         */
        set_new_genesis: TxDescriptor<Anonymize<Iemqna2uucuei9>>;
        /**
         * Report fork voting equivocation. This method will verify the equivocation proof
         * and validate the given key ownership proof against the extracted offender.
         * If both are valid, the offence will be reported.
         */
        report_fork_voting: TxDescriptor<Anonymize<Ibhcht7ff9e575>>;
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
        report_fork_voting_unsigned: TxDescriptor<Anonymize<Ibhcht7ff9e575>>;
        /**
         * Report future block voting equivocation. This method will verify the equivocation proof
         * and validate the given key ownership proof against the extracted offender.
         * If both are valid, the offence will be reported.
         */
        report_future_block_voting: TxDescriptor<Anonymize<Ibtvrus3gmn010>>;
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
        report_future_block_voting_unsigned: TxDescriptor<Anonymize<Ibtvrus3gmn010>>;
    };
    GovernedMap: {
        /**
         * Inherent to register any changes in the state of the Governed Map on Cardano compared to the state currently stored in the pallet.
         */
        register_changes: TxDescriptor<Anonymize<Ib8ul16km22fkf>>;
        /**
         * Changes the address of the Governed Map validator used for observation.
         *
         * This extrinsic must be run either using `sudo` or some other chain governance mechanism.
         */
        set_main_chain_scripts: TxDescriptor<Anonymize<I5aub141nk0cu6>>;
    };
};
type IEvent = {
    System: {
        /**
         * An extrinsic completed successfully.
         */
        ExtrinsicSuccess: PlainDescriptor<Anonymize<Ia82mnkmeo2rhc>>;
        /**
         * An extrinsic failed.
         */
        ExtrinsicFailed: PlainDescriptor<Anonymize<I870h7pfmbjt5m>>;
        /**
         * `:code` was updated.
         */
        CodeUpdated: PlainDescriptor<undefined>;
        /**
         * A new account was created.
         */
        NewAccount: PlainDescriptor<Anonymize<Icbccs0ug47ilf>>;
        /**
         * An account was reaped.
         */
        KilledAccount: PlainDescriptor<Anonymize<Icbccs0ug47ilf>>;
        /**
         * On on-chain remark happened.
         */
        Remarked: PlainDescriptor<Anonymize<I855j4i3kr8ko1>>;
        /**
         * An upgrade was authorized.
         */
        UpgradeAuthorized: PlainDescriptor<Anonymize<Ibgl04rn6nbfm6>>;
        /**
         * An invalid authorized upgrade was rejected while trying to apply it.
         */
        RejectedInvalidAuthorizedUpgrade: PlainDescriptor<Anonymize<I7f4oosp2hcgjo>>;
    };
    Grandpa: {
        /**
         * New authority set has been applied.
         */
        NewAuthorities: PlainDescriptor<Anonymize<I5768ac424h061>>;
        /**
         * Current authority set has been paused.
         */
        Paused: PlainDescriptor<undefined>;
        /**
         * Current authority set has been resumed.
         */
        Resumed: PlainDescriptor<undefined>;
    };
    Midnight: {
        /**
         * A contract was called.
         */
        ContractCall: PlainDescriptor<Anonymize<I5o0in87i4h9qh>>;
        /**
         * A contract has been deployed.
         */
        ContractDeploy: PlainDescriptor<Anonymize<I5o0in87i4h9qh>>;
        /**
         * A transaction has been applied (both the guaranteed and conditional part).
         */
        TxApplied: PlainDescriptor<FixedSizeBinary<32>>;
        /**
         * Contract ownership changes to enable snark upgrades
         */
        ContractMaintain: PlainDescriptor<Anonymize<I5o0in87i4h9qh>>;
        /**
         * New payout minted.
         */
        PayoutMinted: PlainDescriptor<Anonymize<I7nfq6ftas0rri>>;
        /**
         * Payout was claimed.
         */
        ClaimMint: PlainDescriptor<Anonymize<I9qdvp794ab9dj>>;
        /**
         * Unshielded Tokens Trasfers
         */
        UnshieldedTokens: PlainDescriptor<Anonymize<I3seth7anm0bu2>>;
        /**
         * Partial Success.
         */
        TxPartialSuccess: PlainDescriptor<FixedSizeBinary<32>>;
    };
    Balances: {
        /**
         * An account was created with some free balance.
         */
        Endowed: PlainDescriptor<Anonymize<Icv68aq8841478>>;
        /**
         * An account was removed whose balance was non-zero but below ExistentialDeposit,
         * resulting in an outright loss.
         */
        DustLost: PlainDescriptor<Anonymize<Ic262ibdoec56a>>;
        /**
         * Transfer succeeded.
         */
        Transfer: PlainDescriptor<Anonymize<Iflcfm9b6nlmdd>>;
        /**
         * A balance was set by root.
         */
        BalanceSet: PlainDescriptor<Anonymize<Ijrsf4mnp3eka>>;
        /**
         * Some balance was reserved (moved from free to reserved).
         */
        Reserved: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some balance was unreserved (moved from reserved to free).
         */
        Unreserved: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some balance was moved from the reserve of the first account to the second account.
         * Final argument indicates the destination balance type.
         */
        ReserveRepatriated: PlainDescriptor<Anonymize<I8tjvj9uq4b7hi>>;
        /**
         * Some amount was deposited (e.g. for transaction fees).
         */
        Deposit: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some amount was withdrawn from the account (e.g. for transaction fees).
         */
        Withdraw: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some amount was removed from the account (e.g. for misbehavior).
         */
        Slashed: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some amount was minted into an account.
         */
        Minted: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some amount was burned from an account.
         */
        Burned: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some amount was suspended from an account (it can be restored later).
         */
        Suspended: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some amount was restored into an account.
         */
        Restored: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * An account was upgraded.
         */
        Upgraded: PlainDescriptor<Anonymize<I4cbvqmqadhrea>>;
        /**
         * Total issuance was increased by `amount`, creating a credit to be balanced.
         */
        Issued: PlainDescriptor<Anonymize<I3qt1hgg4djhgb>>;
        /**
         * Total issuance was decreased by `amount`, creating a debt to be balanced.
         */
        Rescinded: PlainDescriptor<Anonymize<I3qt1hgg4djhgb>>;
        /**
         * Some balance was locked.
         */
        Locked: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some balance was unlocked.
         */
        Unlocked: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some balance was frozen.
         */
        Frozen: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * Some balance was thawed.
         */
        Thawed: PlainDescriptor<Anonymize<Id5fm4p8lj5qgi>>;
        /**
         * The `TotalIssuance` was forcefully changed.
         */
        TotalIssuanceForced: PlainDescriptor<Anonymize<I4fooe9dun9o0t>>;
    };
    Sudo: {
        /**
         * A sudo call just took place.
         */
        Sudid: PlainDescriptor<Anonymize<I4npgd22g8nrk7>>;
        /**
         * The sudo key has been updated.
         */
        KeyChanged: PlainDescriptor<Anonymize<I5rtkmhm2dng4u>>;
        /**
         * The key was permanently removed.
         */
        KeyRemoved: PlainDescriptor<undefined>;
        /**
         * A [sudo_as](Pallet::sudo_as) call just took place.
         */
        SudoAsDone: PlainDescriptor<Anonymize<I4npgd22g8nrk7>>;
    };
    RuntimeUpgrade: {
        /**
         * Signal an issue when attempting a runtime upgrade, in a context where pallet errors are not accessible
         */
        CouldNotScheduleRuntimeUpgrade: PlainDescriptor<Anonymize<I3f33lun6ld0ef>>;
        /**
         * No votes were made this round
         */
        NoVotes: PlainDescriptor<undefined>;
        /**
         * Code upgrade managed by this pallet was scheduled
         */
        UpgradeScheduled: PlainDescriptor<Anonymize<I70pruthef0ilh>>;
        /**
         * Validators could not agree on an upgrade, and voting will be reset
         */
        NoConsensusOnUpgrade: PlainDescriptor<undefined>;
        /**
         * Upgrade was not performed because a preimage of the upgrade request was not found
         */
        NoUpgradePreimageMissing: PlainDescriptor<Anonymize<I6tccms8877uoj>>;
        /**
         * Upgrade was not performed because the request for its preimage was not found
         */
        NoUpgradePreimageNotRequested: PlainDescriptor<Anonymize<I6tccms8877uoj>>;
        /**
         * An upgrade was attempted, but the call size exceeded the configured bounds
         */
        UpgradeCallTooLarge: PlainDescriptor<Anonymize<I3f33lun6ld0ef>>;
        /**
         * A validator has voted on an upgrade
         */
        Voted: PlainDescriptor<Anonymize<I1ohl1lmd4r5j8>>;
    };
    NativeTokenManagement: {
        /**
         * Signals that a new native token transfer has been processed by the pallet
         */
        TokensTransfered: PlainDescriptor<bigint>;
    };
    NativeTokenObservation: {
        /**
        
         */
        Added: PlainDescriptor<Anonymize<I7dt739idcq9qk>>;
        /**
         * Tried to remove an element, but it was not found in the list of registrations
         */
        AttemptedRemoveNonexistantElement: PlainDescriptor<Anonymize<Ic1blifjtonodb>>;
        /**
         * Could not add registration
         */
        CouldNotAddRegistration: PlainDescriptor<undefined>;
        /**
        
         */
        DuplicatedRegistration: PlainDescriptor<Anonymize<I7dt739idcq9qk>>;
        /**
        
         */
        InvalidCardanoAddress: PlainDescriptor<undefined>;
        /**
        
         */
        InvalidDustAddress: PlainDescriptor<undefined>;
        /**
        
         */
        Registered: PlainDescriptor<Anonymize<I7dt739idcq9qk>>;
        /**
         * Removed registrations
         */
        Removed: PlainDescriptor<Anonymize<I4nibra9bv0ahp>>;
        /**
         * Removed single registration in order to add a new registration in order to respect length bound of registration list
         */
        RemovedOld: PlainDescriptor<Anonymize<I4nibra9bv0ahp>>;
        /**
         * System transaction - the `SystemTx` struct is defined in the Node for now, but this event will contain a Ledger System Transaction
         */
        SystemTx: PlainDescriptor<Anonymize<I9i3o8bp584ej>>;
    };
    Preimage: {
        /**
         * A preimage has been noted.
         */
        Noted: PlainDescriptor<Anonymize<I1jm8m1rh9e20v>>;
        /**
         * A preimage has been requested.
         */
        Requested: PlainDescriptor<Anonymize<I1jm8m1rh9e20v>>;
        /**
         * A preimage has ben cleared.
         */
        Cleared: PlainDescriptor<Anonymize<I1jm8m1rh9e20v>>;
    };
    MultiBlockMigrations: {
        /**
         * A Runtime upgrade started.
         *
         * Its end is indicated by `UpgradeCompleted` or `UpgradeFailed`.
         */
        UpgradeStarted: PlainDescriptor<Anonymize<If1co0pilmi7oq>>;
        /**
         * The current runtime upgrade completed.
         *
         * This implies that all of its migrations completed successfully as well.
         */
        UpgradeCompleted: PlainDescriptor<undefined>;
        /**
         * Runtime upgrade failed.
         *
         * This is very bad and will require governance intervention.
         */
        UpgradeFailed: PlainDescriptor<undefined>;
        /**
         * A migration was skipped since it was already executed in the past.
         */
        MigrationSkipped: PlainDescriptor<Anonymize<I666bl2fqjkejo>>;
        /**
         * A migration progressed.
         */
        MigrationAdvanced: PlainDescriptor<Anonymize<Iae74gjak1qibn>>;
        /**
         * A Migration completed.
         */
        MigrationCompleted: PlainDescriptor<Anonymize<Iae74gjak1qibn>>;
        /**
         * A Migration failed.
         *
         * This implies that the whole upgrade failed and governance intervention is required.
         */
        MigrationFailed: PlainDescriptor<Anonymize<Iae74gjak1qibn>>;
        /**
         * The set of historical migrations has been cleared.
         */
        HistoricCleared: PlainDescriptor<Anonymize<I3escdojpj0551>>;
    };
    PalletSession: {
        /**
         * New session has happened. Note that the argument is the session index, not the
         * block number as the type might suggest.
         */
        NewSession: PlainDescriptor<Anonymize<I2hq50pu2kdjpo>>;
        /**
         * Validator has been disabled.
         */
        ValidatorDisabled: PlainDescriptor<Anonymize<I9acqruh7322g2>>;
        /**
         * Validator has been re-enabled.
         */
        ValidatorReenabled: PlainDescriptor<Anonymize<I9acqruh7322g2>>;
    };
    Scheduler: {
        /**
         * Scheduled some task.
         */
        Scheduled: PlainDescriptor<Anonymize<I5n4sebgkfr760>>;
        /**
         * Canceled some task.
         */
        Canceled: PlainDescriptor<Anonymize<I5n4sebgkfr760>>;
        /**
         * Dispatched some task.
         */
        Dispatched: PlainDescriptor<Anonymize<Ifhslud4mm5dob>>;
        /**
         * Set a retry configuration for some task.
         */
        RetrySet: PlainDescriptor<Anonymize<Ia3c82eadg79bj>>;
        /**
         * Cancel a retry configuration for some task.
         */
        RetryCancelled: PlainDescriptor<Anonymize<Ienusoeb625ftq>>;
        /**
         * The call for the provided hash was not found so the task has been aborted.
         */
        CallUnavailable: PlainDescriptor<Anonymize<Ienusoeb625ftq>>;
        /**
         * The given task was unable to be renewed since the agenda is full at that block.
         */
        PeriodicFailed: PlainDescriptor<Anonymize<Ienusoeb625ftq>>;
        /**
         * The given task was unable to be retried since the agenda is full at that block or there
         * was not enough weight to reschedule it.
         */
        RetryFailed: PlainDescriptor<Anonymize<Ienusoeb625ftq>>;
        /**
         * The given task can never be executed since it is overweight.
         */
        PermanentlyOverweight: PlainDescriptor<Anonymize<Ienusoeb625ftq>>;
        /**
         * Agenda is incomplete from `when`.
         */
        AgendaIncomplete: PlainDescriptor<Anonymize<Ibtsa3docbr9el>>;
    };
    TxPause: {
        /**
         * This pallet, or a specific call is now paused.
         */
        CallPaused: PlainDescriptor<Anonymize<Iba7pefg0d11kh>>;
        /**
         * This pallet, or a specific call is now unpaused.
         */
        CallUnpaused: PlainDescriptor<Anonymize<Iba7pefg0d11kh>>;
    };
    Session: {
        /**
         * New session has happened. Note that the argument is the session index, not the
         * block number as the type might suggest.
         */
        NewSession: PlainDescriptor<Anonymize<I2hq50pu2kdjpo>>;
    };
};
type IError = {
    System: {
        /**
         * The name of specification does not match between the current runtime
         * and the new runtime.
         */
        InvalidSpecName: PlainDescriptor<undefined>;
        /**
         * The specification version is not allowed to decrease between the current runtime
         * and the new runtime.
         */
        SpecVersionNeedsToIncrease: PlainDescriptor<undefined>;
        /**
         * Failed to extract the runtime version from the new runtime.
         *
         * Either calling `Core_version` or decoding `RuntimeVersion` failed.
         */
        FailedToExtractRuntimeVersion: PlainDescriptor<undefined>;
        /**
         * Suicide called when the account has non-default composite data.
         */
        NonDefaultComposite: PlainDescriptor<undefined>;
        /**
         * There is a non-zero reference count preventing the account from being purged.
         */
        NonZeroRefCount: PlainDescriptor<undefined>;
        /**
         * The origin filter prevent the call to be dispatched.
         */
        CallFiltered: PlainDescriptor<undefined>;
        /**
         * A multi-block migration is ongoing and prevents the current code from being replaced.
         */
        MultiBlockMigrationsOngoing: PlainDescriptor<undefined>;
        /**
         * No upgrade authorized.
         */
        NothingAuthorized: PlainDescriptor<undefined>;
        /**
         * The submitted code is not authorized.
         */
        Unauthorized: PlainDescriptor<undefined>;
    };
    Grandpa: {
        /**
         * Attempt to signal GRANDPA pause when the authority set isn't live
         * (either paused or already pending pause).
         */
        PauseFailed: PlainDescriptor<undefined>;
        /**
         * Attempt to signal GRANDPA resume when the authority set isn't paused
         * (either live or already pending resume).
         */
        ResumeFailed: PlainDescriptor<undefined>;
        /**
         * Attempt to signal GRANDPA change with one already pending.
         */
        ChangePending: PlainDescriptor<undefined>;
        /**
         * Cannot signal forced change so soon after last.
         */
        TooSoon: PlainDescriptor<undefined>;
        /**
         * A key ownership proof provided as part of an equivocation report is invalid.
         */
        InvalidKeyOwnershipProof: PlainDescriptor<undefined>;
        /**
         * An equivocation proof provided as part of an equivocation report is invalid.
         */
        InvalidEquivocationProof: PlainDescriptor<undefined>;
        /**
         * A given equivocation report is valid but already previously reported.
         */
        DuplicateOffenceReport: PlainDescriptor<undefined>;
    };
    Midnight: {
        /**
        
         */
        NewStateOutOfBounds: PlainDescriptor<undefined>;
        /**
        
         */
        Deserialization: PlainDescriptor<Anonymize<Ihhhb06ltk59c>>;
        /**
        
         */
        Serialization: PlainDescriptor<Anonymize<Ichp8s4hhgp7ug>>;
        /**
        
         */
        Transaction: PlainDescriptor<Anonymize<Ic4hvjmrliv95i>>;
        /**
        
         */
        LedgerCacheError: PlainDescriptor<undefined>;
        /**
        
         */
        NoLedgerState: PlainDescriptor<undefined>;
        /**
        
         */
        LedgerStateScaleDecodingError: PlainDescriptor<undefined>;
        /**
        
         */
        ContractCallCostError: PlainDescriptor<undefined>;
    };
    Balances: {
        /**
         * Vesting balance too high to send value.
         */
        VestingBalance: PlainDescriptor<undefined>;
        /**
         * Account liquidity restrictions prevent withdrawal.
         */
        LiquidityRestrictions: PlainDescriptor<undefined>;
        /**
         * Balance too low to send value.
         */
        InsufficientBalance: PlainDescriptor<undefined>;
        /**
         * Value too low to create account due to existential deposit.
         */
        ExistentialDeposit: PlainDescriptor<undefined>;
        /**
         * Transfer/payment would kill account.
         */
        Expendability: PlainDescriptor<undefined>;
        /**
         * A vesting schedule already exists for this account.
         */
        ExistingVestingSchedule: PlainDescriptor<undefined>;
        /**
         * Beneficiary account must pre-exist.
         */
        DeadAccount: PlainDescriptor<undefined>;
        /**
         * Number of named reserves exceed `MaxReserves`.
         */
        TooManyReserves: PlainDescriptor<undefined>;
        /**
         * Number of holds exceed `VariantCountOf<T::RuntimeHoldReason>`.
         */
        TooManyHolds: PlainDescriptor<undefined>;
        /**
         * Number of freezes exceed `MaxFreezes`.
         */
        TooManyFreezes: PlainDescriptor<undefined>;
        /**
         * The issuance cannot be modified since it is already deactivated.
         */
        IssuanceDeactivated: PlainDescriptor<undefined>;
        /**
         * The delta cannot be zero.
         */
        DeltaZero: PlainDescriptor<undefined>;
    };
    Sudo: {
        /**
         * Sender must be the Sudo account.
         */
        RequireSudo: PlainDescriptor<undefined>;
    };
    SessionCommitteeManagement: {
        /**
         * [Pallet::set] has been called with epoch number that is not current epoch + 1
         */
        InvalidEpoch: PlainDescriptor<undefined>;
        /**
         * [Pallet::set] has been called a second time for the same next epoch
         */
        NextCommitteeAlreadySet: PlainDescriptor<undefined>;
    };
    RuntimeUpgrade: {
        /**
         * Inherent transaction requires current authority information, but this was not able to be retrived from AURA
         */
        CouldNotLoadCurrentAuthority: PlainDescriptor<undefined>;
        /**
         * An error occurred when calling a runtime upgrade
         */
        RuntimeUpgradeError: PlainDescriptor<undefined>;
        /**
         * Limit for votes was exceeded
         */
        VoteThresholdExceeded: PlainDescriptor<undefined>;
    };
    NativeTokenManagement: {
        /**
         * Indicates that the inherent was called while there was no main chain scripts set in the
         * pallet's storage. This is indicative of a programming bug.
         */
        CalledWithoutConfiguration: PlainDescriptor<undefined>;
        /**
         * Indicates that the inherent was called a second time in the same block
         */
        TransferAlreadyHandled: PlainDescriptor<undefined>;
    };
    NativeTokenObservation: {
        /**
         * A Cardano Wallet address was sent, but was longer than expected
         */
        MaxCardanoAddrLengthExceeded: PlainDescriptor<undefined>;
        /**
        
         */
        MaxRegistrationsExceeded: PlainDescriptor<undefined>;
    };
    Preimage: {
        /**
         * Preimage is too large to store on-chain.
         */
        TooBig: PlainDescriptor<undefined>;
        /**
         * Preimage has already been noted on-chain.
         */
        AlreadyNoted: PlainDescriptor<undefined>;
        /**
         * The user is not authorized to perform this action.
         */
        NotAuthorized: PlainDescriptor<undefined>;
        /**
         * The preimage cannot be removed since it has not yet been noted.
         */
        NotNoted: PlainDescriptor<undefined>;
        /**
         * A preimage may not be removed when there are outstanding requests.
         */
        Requested: PlainDescriptor<undefined>;
        /**
         * The preimage request cannot be removed since no outstanding requests exist.
         */
        NotRequested: PlainDescriptor<undefined>;
        /**
         * More than `MAX_HASH_UPGRADE_BULK_COUNT` hashes were requested to be upgraded at once.
         */
        TooMany: PlainDescriptor<undefined>;
        /**
         * Too few hashes were requested to be upgraded (i.e. zero).
         */
        TooFew: PlainDescriptor<undefined>;
    };
    MultiBlockMigrations: {
        /**
         * The operation cannot complete since some MBMs are ongoing.
         */
        Ongoing: PlainDescriptor<undefined>;
    };
    PalletSession: {
        /**
         * Invalid ownership proof.
         */
        InvalidProof: PlainDescriptor<undefined>;
        /**
         * No associated validator ID for account.
         */
        NoAssociatedValidatorId: PlainDescriptor<undefined>;
        /**
         * Registered duplicate key.
         */
        DuplicatedKey: PlainDescriptor<undefined>;
        /**
         * No keys are associated with this account.
         */
        NoKeys: PlainDescriptor<undefined>;
        /**
         * Key setting account is not live, so it's impossible to associate keys.
         */
        NoAccount: PlainDescriptor<undefined>;
    };
    Scheduler: {
        /**
         * Failed to schedule a call
         */
        FailedToSchedule: PlainDescriptor<undefined>;
        /**
         * Cannot find the scheduled call.
         */
        NotFound: PlainDescriptor<undefined>;
        /**
         * Given target block number is in the past.
         */
        TargetBlockNumberInPast: PlainDescriptor<undefined>;
        /**
         * Reschedule failed because it does not change scheduled time.
         */
        RescheduleNoChange: PlainDescriptor<undefined>;
        /**
         * Attempt to use a non-named function on a named task.
         */
        Named: PlainDescriptor<undefined>;
    };
    TxPause: {
        /**
         * The call is paused.
         */
        IsPaused: PlainDescriptor<undefined>;
        /**
         * The call is unpaused.
         */
        IsUnpaused: PlainDescriptor<undefined>;
        /**
         * The call is whitelisted and cannot be paused.
         */
        Unpausable: PlainDescriptor<undefined>;
        /**
        
         */
        NotFound: PlainDescriptor<undefined>;
    };
    Beefy: {
        /**
         * A key ownership proof provided as part of an equivocation report is invalid.
         */
        InvalidKeyOwnershipProof: PlainDescriptor<undefined>;
        /**
         * A double voting proof provided as part of an equivocation report is invalid.
         */
        InvalidDoubleVotingProof: PlainDescriptor<undefined>;
        /**
         * A fork voting proof provided as part of an equivocation report is invalid.
         */
        InvalidForkVotingProof: PlainDescriptor<undefined>;
        /**
         * A future block voting proof provided as part of an equivocation report is invalid.
         */
        InvalidFutureBlockVotingProof: PlainDescriptor<undefined>;
        /**
         * The session of the equivocation proof is invalid
         */
        InvalidEquivocationProofSession: PlainDescriptor<undefined>;
        /**
         * A given equivocation report is valid but already previously reported.
         */
        DuplicateOffenceReport: PlainDescriptor<undefined>;
        /**
         * Submitted configuration is invalid.
         */
        InvalidConfiguration: PlainDescriptor<undefined>;
    };
    GovernedMap: {
        /**
         * Signals that the inherent has been called again in the same block
         */
        InherentCalledTwice: PlainDescriptor<undefined>;
        /**
         * MainChainScript is not set, registration of changes is not allowed
         */
        MainChainScriptNotSet: PlainDescriptor<undefined>;
    };
};
type IConstants = {
    System: {
        /**
         * Block & extrinsics weights: base values and limits.
         */
        BlockWeights: PlainDescriptor<Anonymize<In7a38730s6qs>>;
        /**
         * The maximum length of a block (in bytes).
         */
        BlockLength: PlainDescriptor<Anonymize<If15el53dd76v9>>;
        /**
         * Maximum number of block number to block hash mappings to keep (oldest pruned first).
         */
        BlockHashCount: PlainDescriptor<number>;
        /**
         * The weight of runtime database operations the runtime can invoke.
         */
        DbWeight: PlainDescriptor<Anonymize<I9s0ave7t0vnrk>>;
        /**
         * Get the chain's in-code version.
         */
        Version: PlainDescriptor<Anonymize<I4fo08joqmcqnm>>;
        /**
         * The designated SS58 prefix of this chain.
         *
         * This replaces the "ss58Format" property declared in the chain spec. Reason is
         * that the runtime should know about the prefix in order to make use of it as
         * an identifier of the chain.
         */
        SS58Prefix: PlainDescriptor<number>;
    };
    Timestamp: {
        /**
         * The minimum period between blocks.
         *
         * Be aware that this is different to the *expected* period that the block production
         * apparatus provides. Your chosen consensus system will generally work with this to
         * determine a sensible block time. For example, in the Aura pallet it will be double this
         * period on default settings.
         */
        MinimumPeriod: PlainDescriptor<bigint>;
    };
    Aura: {
        /**
         * The slot duration Aura should run with, expressed in milliseconds.
         * The effective value of this type should not change while the chain is running.
         *
         * For backwards compatibility either use [`MinimumPeriodTimesTwo`] or a const.
         */
        SlotDuration: PlainDescriptor<bigint>;
    };
    Grandpa: {
        /**
         * Max Authorities in use
         */
        MaxAuthorities: PlainDescriptor<number>;
        /**
         * The maximum number of nominators for each validator.
         */
        MaxNominators: PlainDescriptor<number>;
        /**
         * The maximum number of entries to keep in the set id to session index mapping.
         *
         * Since the `SetIdSession` map is only used for validating equivocations this
         * value should relate to the bonding duration of whatever staking system is
         * being used (if any). If equivocation handling is not enabled then this value
         * can be zero.
         */
        MaxSetIdSessionEntries: PlainDescriptor<bigint>;
    };
    Balances: {
        /**
         * The minimum amount required to keep an account open. MUST BE GREATER THAN ZERO!
         *
         * If you *really* need it to be zero, you can enable the feature `insecure_zero_ed` for
         * this pallet. However, you do so at your own risk: this will open up a major DoS vector.
         * In case you have multiple sources of provider references, you may also get unexpected
         * behaviour if you set this to zero.
         *
         * Bottom line: Do yourself a favour and make it at least one!
         */
        ExistentialDeposit: PlainDescriptor<bigint>;
        /**
         * The maximum number of locks that should exist on an account.
         * Not strictly enforced, but used for weight estimation.
         *
         * Use of locks is deprecated in favour of freezes. See `https://github.com/paritytech/substrate/pull/12951/`
         */
        MaxLocks: PlainDescriptor<number>;
        /**
         * The maximum number of named reserves that can exist on an account.
         *
         * Use of reserves is deprecated in favour of holds. See `https://github.com/paritytech/substrate/pull/12951/`
         */
        MaxReserves: PlainDescriptor<number>;
        /**
         * The maximum number of individual freeze locks that can exist on an account at any time.
         */
        MaxFreezes: PlainDescriptor<number>;
    };
    SessionCommitteeManagement: {
        /**
         * Maximum amount of validators.
         */
        MaxValidators: PlainDescriptor<number>;
    };
    RuntimeUpgrade: {
        /**
         * The Lottery's pallet id
         */
        PalletId: PlainDescriptor<FixedSizeBinary<8>>;
        /**
         * Number of blocks before any given scheduled upgrade occurs.
         */
        UpgradeDelay: PlainDescriptor<number>;
        /**
         * Percentage of the current validator set who must vote on the upgrade in order for it to pass
         */
        UpgradeVoteThreshold: PlainDescriptor<number>;
    };
    NativeTokenObservation: {
        /**
        
         */
        MaxRegistrationsPerCardanoAddress: PlainDescriptor<number>;
    };
    MultiBlockMigrations: {
        /**
         * The maximal length of an encoded cursor.
         *
         * A good default needs to selected such that no migration will ever have a cursor with MEL
         * above this limit. This is statically checked in `integrity_test`.
         */
        CursorMaxLen: PlainDescriptor<number>;
        /**
         * The maximal length of an encoded identifier.
         *
         * A good default needs to selected such that no migration will ever have an identifier
         * with MEL above this limit. This is statically checked in `integrity_test`.
         */
        IdentifierMaxLen: PlainDescriptor<number>;
    };
    Scheduler: {
        /**
         * The maximum weight that may be scheduled per block for any dispatchables.
         */
        MaximumWeight: PlainDescriptor<Anonymize<I4q39t5hn830vp>>;
        /**
         * The maximum number of scheduled calls in the queue for a single block.
         *
         * NOTE:
         * + Dependent pallets' benchmarks might require a higher limit for the setting. Set a
         * higher limit under `runtime-benchmarks` feature.
         */
        MaxScheduledPerBlock: PlainDescriptor<number>;
    };
    TxPause: {
        /**
         * Maximum length for pallet name and call name SCALE encoded string names.
         *
         * TOO LONG NAMES WILL BE TREATED AS PAUSED.
         */
        MaxNameLen: PlainDescriptor<number>;
    };
    Beefy: {
        /**
         * The maximum number of authorities that can be added.
         */
        MaxAuthorities: PlainDescriptor<number>;
        /**
         * The maximum number of nominators for each validator.
         */
        MaxNominators: PlainDescriptor<number>;
        /**
         * The maximum number of entries to keep in the set id to session index mapping.
         *
         * Since the `SetIdSession` map is only used for validating equivocations this
         * value should relate to the bonding duration of whatever staking system is
         * being used (if any). If equivocation handling is not enabled then this value
         * can be zero.
         */
        MaxSetIdSessionEntries: PlainDescriptor<bigint>;
    };
    GovernedMap: {
        /**
         * Maximum number of changes that can be registered in a single inherent.
         *
         * This value *must* be high enough for all changes to be registered in one block.
         * Setting this to a value higher than the total number of parameters in the Governed Map guarantees that.
         */
        MaxChanges: PlainDescriptor<number>;
        /**
         * Maximum length of the key in the Governed Map in bytes.
         *
         * This value *must* be high enough not to be exceeded by any key stored on Cardano.
         */
        MaxKeyLength: PlainDescriptor<number>;
        /**
         * Maximum length of data stored under a single key in the Governed Map
         *
         * This value *must* be high enough not to be exceeded by any value stored on Cardano.
         */
        MaxValueLength: PlainDescriptor<number>;
    };
};
type IViewFns = {};
type IRuntimeCalls = {
    /**
     * Runtime API exposing configuration and initialization status of the Native Token Management pallet
     */
    NativeTokenManagementApi: {
        /**
         * Returns the current main chain scripts configured in the pallet or [None] if they are not set.
         */
        get_main_chain_scripts: RuntimeDescriptor<[], Anonymize<Idacla3pi5jort>>;
        /**
         * Gets current initializaion status and set it to `true` afterwards. This check is used to
         * determine whether historical data from the beginning of main chain should be queried.
         */
        initialized: RuntimeDescriptor<[], boolean>;
    };
    /**
     * API to interact with `RuntimeGenesisConfig` for the runtime
     */
    GenesisBuilder: {
        /**
         * Build `RuntimeGenesisConfig` from a JSON blob not using any defaults and store it in the
         * storage.
         *
         * In the case of a FRAME-based runtime, this function deserializes the full
         * `RuntimeGenesisConfig` from the given JSON blob and puts it into the storage. If the
         * provided JSON blob is incorrect or incomplete or the deserialization fails, an error
         * is returned.
         *
         * Please note that provided JSON blob must contain all `RuntimeGenesisConfig` fields, no
         * defaults will be used.
         */
        build_state: RuntimeDescriptor<[json: Binary], Anonymize<Ie9sr1iqcg3cgm>>;
        /**
         * Returns a JSON blob representation of the built-in `RuntimeGenesisConfig` identified by
         * `id`.
         *
         * If `id` is `None` the function should return JSON blob representation of the default
         * `RuntimeGenesisConfig` struct of the runtime. Implementation must provide default
         * `RuntimeGenesisConfig`.
         *
         * Otherwise function returns a JSON representation of the built-in, named
         * `RuntimeGenesisConfig` preset identified by `id`, or `None` if such preset does not
         * exist. Returned `Vec<u8>` contains bytes of JSON blob (patch) which comprises a list of
         * (potentially nested) key-value pairs that are intended for customizing the default
         * runtime genesis config. The patch shall be merged (rfc7386) with the JSON representation
         * of the default `RuntimeGenesisConfig` to create a comprehensive genesis config that can
         * be used in `build_state` method.
         */
        get_preset: RuntimeDescriptor<[id: Anonymize<I1mqgk2tmnn9i2>], Anonymize<Iabpgqcjikia83>>;
        /**
         * Returns a list of identifiers for available builtin `RuntimeGenesisConfig` presets.
         *
         * The presets from the list can be queried with [`GenesisBuilder::get_preset`] method. If
         * no named presets are provided by the runtime the list is empty.
         */
        preset_names: RuntimeDescriptor<[], Anonymize<I6lr8sctk0bi4e>>;
    };
    /**
     * The `Core` runtime api that every Substrate runtime needs to implement.
     */
    Core: {
        /**
         * Returns the version of the runtime.
         */
        version: RuntimeDescriptor<[], Anonymize<I4fo08joqmcqnm>>;
        /**
         * Execute the given block.
         */
        execute_block: RuntimeDescriptor<[block: Anonymize<Iaqet9jc3ihboe>], undefined>;
        /**
         * Initialize a block with the given header and return the runtime executive mode.
         */
        initialize_block: RuntimeDescriptor<[header: Anonymize<Ic952bubvq4k7d>], Anonymize<I2v50gu3s1aqk6>>;
    };
    /**
    
     */
    MidnightRuntimeApi: {
        /**
        
         */
        get_contract_state: RuntimeDescriptor<[contract_address: Binary], Anonymize<I4mqaaf6ie66ve>>;
        /**
        
         */
        get_decoded_transaction: RuntimeDescriptor<[transaction_bytes: Binary], Anonymize<Ia1pome47r2aq5>>;
        /**
        
         */
        get_zswap_chain_state: RuntimeDescriptor<[contract_address: Binary], Anonymize<I4mqaaf6ie66ve>>;
        /**
        
         */
        get_network_id: RuntimeDescriptor<[], Binary>;
        /**
        
         */
        get_ledger_version: RuntimeDescriptor<[], Binary>;
        /**
        
         */
        get_unclaimed_amount: RuntimeDescriptor<[beneficiary: Binary], Anonymize<Iehtf0ht2ndj33>>;
        /**
        
         */
        get_ledger_parameters: RuntimeDescriptor<[], Anonymize<I4mqaaf6ie66ve>>;
        /**
        
         */
        get_transaction_cost: RuntimeDescriptor<[transaction_bytes: Binary], Anonymize<Ieduh03298o0nh>>;
        /**
        
         */
        get_zswap_state_root: RuntimeDescriptor<[], Anonymize<I4mqaaf6ie66ve>>;
    };
    /**
    
     */
    UpgradeApi: {
        /**
        
         */
        get_current_version_info: RuntimeDescriptor<[], Anonymize<I4p5t2krb1gmvp>>;
    };
    /**
     * The `Metadata` api trait that returns metadata for the runtime.
     */
    Metadata: {
        /**
         * Returns the metadata of a runtime.
         */
        metadata: RuntimeDescriptor<[], Binary>;
        /**
         * Returns the metadata at a given version.
         *
         * If the given `version` isn't supported, this will return `None`.
         * Use [`Self::metadata_versions`] to find out about supported metadata version of the runtime.
         */
        metadata_at_version: RuntimeDescriptor<[version: number], Anonymize<Iabpgqcjikia83>>;
        /**
         * Returns the supported metadata versions.
         *
         * This can be used to call `metadata_at_version`.
         */
        metadata_versions: RuntimeDescriptor<[], Anonymize<Icgljjb6j82uhn>>;
    };
    /**
     * The `BlockBuilder` api trait that provides the required functionality for building a block.
     */
    BlockBuilder: {
        /**
         * Apply the given extrinsic.
         *
         * Returns an inclusion outcome which specifies if this extrinsic is included in
         * this block or not.
         */
        apply_extrinsic: RuntimeDescriptor<[extrinsic: Binary], Anonymize<I8k1nb3hdf41md>>;
        /**
         * Finish the current block.
         */
        finalize_block: RuntimeDescriptor<[], Anonymize<Ic952bubvq4k7d>>;
        /**
         * Generate inherent extrinsics. The inherent data will vary from chain to chain.
         */
        inherent_extrinsics: RuntimeDescriptor<[inherent: Anonymize<If7uv525tdvv7a>], Anonymize<Itom7fk49o0c9>>;
        /**
         * Check that the inherents are valid. The inherent data will vary from chain to chain.
         */
        check_inherents: RuntimeDescriptor<[block: Anonymize<Iaqet9jc3ihboe>, data: Anonymize<If7uv525tdvv7a>], Anonymize<I2an1fs2eiebjp>>;
    };
    /**
     * The `TaggedTransactionQueue` api trait for interfering with the transaction queue.
     */
    TaggedTransactionQueue: {
        /**
         * Validate the transaction.
         *
         * This method is invoked by the transaction pool to learn details about given transaction.
         * The implementation should make sure to verify the correctness of the transaction
         * against current state. The given `block_hash` corresponds to the hash of the block
         * that is used as current state.
         *
         * Note that this call may be performed by the pool multiple times and transactions
         * might be verified in any possible order.
         */
        validate_transaction: RuntimeDescriptor<[source: TransactionValidityTransactionSource, tx: Binary, block_hash: FixedSizeBinary<32>], Anonymize<I9ask1o4tfvcvs>>;
    };
    /**
     * The offchain worker api.
     */
    OffchainWorkerApi: {
        /**
         * Starts the off-chain task for given block header.
         */
        offchain_worker: RuntimeDescriptor<[header: Anonymize<Ic952bubvq4k7d>], undefined>;
    };
    /**
     * API necessary for block authorship with aura.
     */
    AuraApi: {
        /**
         * Returns the slot duration for Aura.
         *
         * Currently, only the value provided by this type at genesis will be used.
         */
        slot_duration: RuntimeDescriptor<[], bigint>;
        /**
         * Return the current set of authorities.
         */
        authorities: RuntimeDescriptor<[], Anonymize<Ic5m5lp1oioo8r>>;
    };
    /**
     * API necessary for BEEFY voters.
     */
    BeefyApi: {
        /**
         * Return the block number where BEEFY consensus is enabled/started
         */
        beefy_genesis: RuntimeDescriptor<[], Anonymize<I4arjljr6dpflb>>;
        /**
         * Return the current active BEEFY validator set
         */
        validator_set: RuntimeDescriptor<[], Anonymize<Ifogo2hpqpe6b4>>;
        /**
         * Submits an unsigned extrinsic to report a double voting equivocation. The caller
         * must provide the double voting proof and a key ownership proof
         * (should be obtained using `generate_key_ownership_proof`). The
         * extrinsic will be unsigned and should only be accepted for local
         * authorship (not to be broadcast to the network). This method returns
         * `None` when creation of the extrinsic fails, e.g. if equivocation
         * reporting is disabled for the given runtime (i.e. this method is
         * hardcoded to return `None`). Only useful in an offchain context.
         */
        submit_report_double_voting_unsigned_extrinsic: RuntimeDescriptor<[equivocation_proof: Anonymize<Ifiofttj73fsk1>, key_owner_proof: Binary], boolean>;
        /**
         * Submits an unsigned extrinsic to report a fork voting equivocation. The caller
         * must provide the fork voting proof (the ancestry proof should be obtained using
         * `generate_ancestry_proof`) and a key ownership proof (should be obtained using
         * `generate_key_ownership_proof`). The extrinsic will be unsigned and should only
         * be accepted for local authorship (not to be broadcast to the network). This method
         * returns `None` when creation of the extrinsic fails, e.g. if equivocation
         * reporting is disabled for the given runtime (i.e. this method is
         * hardcoded to return `None`). Only useful in an offchain context.
         */
        submit_report_fork_voting_unsigned_extrinsic: RuntimeDescriptor<[equivocation_proof: Anonymize<I25plekc1moieu>, key_owner_proof: Binary], boolean>;
        /**
         * Submits an unsigned extrinsic to report a future block voting equivocation. The caller
         * must provide the future block voting proof and a key ownership proof
         * (should be obtained using `generate_key_ownership_proof`).
         * The extrinsic will be unsigned and should only be accepted for local
         * authorship (not to be broadcast to the network). This method returns
         * `None` when creation of the extrinsic fails, e.g. if equivocation
         * reporting is disabled for the given runtime (i.e. this method is
         * hardcoded to return `None`). Only useful in an offchain context.
         */
        submit_report_future_block_voting_unsigned_extrinsic: RuntimeDescriptor<[equivocation_proof: Anonymize<I3eao7ea0kppv8>, key_owner_proof: Binary], boolean>;
        /**
         * Generates a proof of key ownership for the given authority in the
         * given set. An example usage of this module is coupled with the
         * session historical module to prove that a given authority key is
         * tied to a given staking identity during a specific session. Proofs
         * of key ownership are necessary for submitting equivocation reports.
         * NOTE: even though the API takes a `set_id` as parameter the current
         * implementations ignores this parameter and instead relies on this
         * method being called at the correct block height, i.e. any point at
         * which the given set id is live on-chain. Future implementations will
         * instead use indexed data through an offchain worker, not requiring
         * older states to be available.
         */
        generate_key_ownership_proof: RuntimeDescriptor<[set_id: bigint, authority_id: FixedSizeBinary<33>], Anonymize<Iabpgqcjikia83>>;
        /**
         * Generates a proof that the `prev_block_number` is part of the canonical chain at
         * `best_known_block_number`.
         */
        generate_ancestry_proof: RuntimeDescriptor<[prev_block_number: number, best_known_block_number: Anonymize<I4arjljr6dpflb>], Anonymize<Iabpgqcjikia83>>;
    };
    /**
     * API to interact with MMR pallet.
     */
    MmrApi: {
        /**
         * Return the on-chain MMR root hash.
         */
        mmr_root: RuntimeDescriptor<[], Anonymize<I7rj2bnb76oko1>>;
        /**
         * Return the number of MMR blocks in the chain.
         */
        mmr_leaf_count: RuntimeDescriptor<[], Anonymize<I4o356o7eq06ms>>;
        /**
         * Generate MMR proof for a series of block numbers. If `best_known_block_number = Some(n)`,
         * use historical MMR state at given block height `n`. Else, use current MMR state.
         */
        generate_proof: RuntimeDescriptor<[block_numbers: Anonymize<Icgljjb6j82uhn>, best_known_block_number: Anonymize<I4arjljr6dpflb>], Anonymize<I46e127tr8ma2h>>;
        /**
         * Verify MMR proof against on-chain MMR for a batch of leaves.
         *
         * Note this function will use on-chain MMR root hash and check if the proof matches the hash.
         * Note, the leaves should be sorted such that corresponding leaves and leaf indices have the
         * same position in both the `leaves` vector and the `leaf_indices` vector contained in the [LeafProof]
         */
        verify_proof: RuntimeDescriptor<[leaves: Anonymize<Itom7fk49o0c9>, proof: Anonymize<I38ee9is0n4jn9>], Anonymize<Ie88mmnuvmuvp5>>;
        /**
         * Verify MMR proof against given root hash for a batch of leaves.
         *
         * Note this function does not require any on-chain storage - the
         * proof is verified against given MMR root hash.
         *
         * Note, the leaves should be sorted such that corresponding leaves and leaf indices have the
         * same position in both the `leaves` vector and the `leaf_indices` vector contained in the [LeafProof]
         */
        verify_proof_stateless: RuntimeDescriptor<[root: FixedSizeBinary<32>, leaves: Anonymize<Itom7fk49o0c9>, proof: Anonymize<I38ee9is0n4jn9>], Anonymize<Ie88mmnuvmuvp5>>;
    };
    /**
     * API useful for BEEFY light clients.
     */
    BeefyMmrApi: {
        /**
         * Return the currently active BEEFY authority set proof.
         */
        authority_set_proof: RuntimeDescriptor<[], Anonymize<Idjett00s2gd>>;
        /**
         * Return the next/queued BEEFY authority set proof.
         */
        next_authority_set_proof: RuntimeDescriptor<[], Anonymize<Idjett00s2gd>>;
    };
    /**
     * Session keys runtime api.
     */
    SessionKeys: {
        /**
         * Generate a set of session keys with optionally using the given seed.
         * The keys should be stored within the keystore exposed via runtime
         * externalities.
         *
         * The seed needs to be a valid `utf8` string.
         *
         * Returns the concatenated SCALE encoded public keys.
         */
        generate_session_keys: RuntimeDescriptor<[seed: Anonymize<Iabpgqcjikia83>], Binary>;
        /**
         * Decode the given public session keys.
         *
         * Returns the list of public raw public keys + key type.
         */
        decode_session_keys: RuntimeDescriptor<[encoded: Binary], Anonymize<Icerf8h8pdu8ss>>;
    };
    /**
     * APIs for integrating the GRANDPA finality gadget into runtimes.
     * This should be implemented on the runtime side.
     *
     * This is primarily used for negotiating authority-set changes for the
     * gadget. GRANDPA uses a signaling model of changing authority sets:
     * changes should be signaled with a delay of N blocks, and then automatically
     * applied in the runtime after those N blocks have passed.
     *
     * The consensus protocol will coordinate the handoff externally.
     */
    GrandpaApi: {
        /**
         * Get the current GRANDPA authorities and weights. This should not change except
         * for when changes are scheduled and the corresponding delay has passed.
         *
         * When called at block B, it will return the set of authorities that should be
         * used to finalize descendants of this block (B+1, B+2, ...). The block B itself
         * is finalized by the authorities from block B-1.
         */
        grandpa_authorities: RuntimeDescriptor<[], Anonymize<I3geksg000c171>>;
        /**
         * Submits an unsigned extrinsic to report an equivocation. The caller
         * must provide the equivocation proof and a key ownership proof
         * (should be obtained using `generate_key_ownership_proof`). The
         * extrinsic will be unsigned and should only be accepted for local
         * authorship (not to be broadcast to the network). This method returns
         * `None` when creation of the extrinsic fails, e.g. if equivocation
         * reporting is disabled for the given runtime (i.e. this method is
         * hardcoded to return `None`). Only useful in an offchain context.
         */
        submit_report_equivocation_unsigned_extrinsic: RuntimeDescriptor<[equivocation_proof: Anonymize<I9puqgoda8ofk4>, key_owner_proof: Binary], boolean>;
        /**
         * Generates a proof of key ownership for the given authority in the
         * given set. An example usage of this module is coupled with the
         * session historical module to prove that a given authority key is
         * tied to a given staking identity during a specific session. Proofs
         * of key ownership are necessary for submitting equivocation reports.
         * NOTE: even though the API takes a `set_id` as parameter the current
         * implementations ignore this parameter and instead rely on this
         * method being called at the correct block height, i.e. any point at
         * which the given set id is live on-chain. Future implementations will
         * instead use indexed data through an offchain worker, not requiring
         * older states to be available.
         */
        generate_key_ownership_proof: RuntimeDescriptor<[set_id: bigint, authority_id: FixedSizeBinary<32>], Anonymize<Iabpgqcjikia83>>;
        /**
         * Get current GRANDPA authority set id.
         */
        current_set_id: RuntimeDescriptor<[], bigint>;
    };
    /**
     * The API to query account nonce.
     */
    AccountNonceApi: {
        /**
         * Get current account nonce of given `AccountId`.
         */
        account_nonce: RuntimeDescriptor<[account: SS58String], number>;
    };
    /**
     * Runtime API for getting information about current Partner Chain slot and epoch
     */
    GetSidechainStatus: {
        /**
         * Returns current Partner Chain slot and epoch
         */
        get_sidechain_status: RuntimeDescriptor<[], Anonymize<I3944ctpg4imgb>>;
    };
    /**
     * Runtime API for retrieving the Partner Chain's genesis UTXO
     */
    GetGenesisUtxo: {
        /**
         * Returns the Partner Chain's genesis UTXO
         */
        genesis_utxo: RuntimeDescriptor<[], Anonymize<Ib7m93p5rn57dr>>;
    };
    /**
     * Runtime API serving slot configuration
     */
    SlotApi: {
        /**
         * Returns the current slot configuration
         */
        slot_config: RuntimeDescriptor<[], Anonymize<Ifpjvh1481i0rn>>;
    };
    /**
     * Runtime API declaration for Session Validator Management
     */
    SessionValidatorManagementApi: {
        /**
         * Returns main chain scripts
         */
        get_main_chain_scripts: RuntimeDescriptor<[], Anonymize<I4r5bhov0m7jqr>>;
        /**
         * Returns next unset [ScEpochNumber]
         */
        get_next_unset_epoch_number: RuntimeDescriptor<[], bigint>;
        /**
         * Returns current committee
         */
        get_current_committee: RuntimeDescriptor<[], Anonymize<I9v2ts6ja6a6bo>>;
        /**
         * Returns next committee
         */
        get_next_committee: RuntimeDescriptor<[], Anonymize<If9hgm7bos9akg>>;
        /**
         * Calculates committee
         */
        calculate_committee: RuntimeDescriptor<[authority_selection_inputs: Anonymize<Ifro4ep1isjk6f>, sidechain_epoch: bigint], Anonymize<I7ihn3rkfc06gt>>;
    };
    /**
     * Runtime API trait for candidate validation
     *
     * When implementing, make sure that the same validation is used here and in the committee selection logic!
     */
    CandidateValidationApi: {
        /**
         * Should validate data provided by registered candidate,
         * and return [RegistrationDataError] in case of validation failure.
         *
         * Should validate:
         * * Aura, GRANDPA, and Partner Chain public keys of the candidate
         * * stake pool signature
         * * sidechain signature
         * * transaction inputs contain correct registration utxo
         */
        validate_registered_candidate_data: RuntimeDescriptor<[mainchain_pub_key: FixedSizeBinary<32>, registration_data: Anonymize<I97al0h8mriqc3>], Anonymize<I6l5pmc2hu9ria>>;
        /**
         * Should validate candidate stake and return [StakeError] in case of validation failure.
         * Should validate stake exists and is positive.
         */
        validate_stake: RuntimeDescriptor<[stake: Anonymize<I35p85j063s0il>], Anonymize<I6k6hm7lt3bcta>>;
        /**
         * Should validate data provided by permissioned candidate,
         * and return [PermissionedCandidateDataError] in case of validation failure.
         *
         * Should validate:
         * * Aura, GRANDPA, and Partner Chain public keys of the candidate
         */
        validate_permissioned_candidate_data: RuntimeDescriptor<[candidate: Anonymize<Ieuflftcnd1rfe>], Anonymize<Inptormg87c56>>;
    };
    /**
    
     */
    NativeTokenObservationApi: {
        /**
         * Get the contract address on Cardano which emits registration mappings in utxo datums
         */
        get_mapping_validator_address: RuntimeDescriptor<[], Binary>;
        /**
         * Get the Cardano native token identifier for the chosen asset
         */
        get_native_token_identifier: RuntimeDescriptor<[], Anonymize<Idkbvh6dahk1v7>>;
        /**
        
         */
        get_next_cardano_position: RuntimeDescriptor<[], Anonymize<I6l15kir0trh70>>;
        /**
        
         */
        get_cardano_block_window_size: RuntimeDescriptor<[], number>;
        /**
        
         */
        get_utxo_capacity_per_block: RuntimeDescriptor<[], number>;
    };
    /**
     * Runtime API exposing data required for the [GovernedMapInherentDataProvider] to operate.
     */
    GovernedMapIDPApi: {
        /**
         * Returns initialization state of the pallet
         */
        is_initialized: RuntimeDescriptor<[], boolean>;
        /**
         * Returns all mappings currently stored in the pallet
         */
        get_current_state: RuntimeDescriptor<[], Anonymize<I2oedcvdsqsu3a>>;
        /**
         * Returns the main chain scripts currently set in the pallet or [None] otherwise
         */
        get_main_chain_scripts: RuntimeDescriptor<[], Anonymize<Ibcsg1mcigvts3>>;
        /**
         * Returns the current version of the pallet, 1-based.
         */
        get_pallet_version: RuntimeDescriptor<[], number>;
    };
};
type IAsset = PlainDescriptor<void>;
export type UndeployedDispatchError = Anonymize<Im2qle9pka0f8>;
type PalletsTypedef = {
    __storage: IStorage;
    __tx: ICalls;
    __event: IEvent;
    __error: IError;
    __const: IConstants;
    __view: IViewFns;
};
type IDescriptors = {
    descriptors: {
        pallets: PalletsTypedef;
        apis: IRuntimeCalls;
    } & Promise<any>;
    metadataTypes: Promise<Uint8Array>;
    asset: IAsset;
    getMetadata: () => Promise<Uint8Array>;
    genesis: string | undefined;
};
declare const _allDescriptors: IDescriptors;
export default _allDescriptors;
export type UndeployedApis = ApisFromDef<IRuntimeCalls>;
export type UndeployedQueries = QueryFromPalletsDef<PalletsTypedef>;
export type UndeployedCalls = TxFromPalletsDef<PalletsTypedef>;
export type UndeployedEvents = EventsFromPalletsDef<PalletsTypedef>;
export type UndeployedErrors = ErrorsFromPalletsDef<PalletsTypedef>;
export type UndeployedConstants = ConstFromPalletsDef<PalletsTypedef>;
export type UndeployedViewFns = ViewFnsFromPalletsDef<PalletsTypedef>;
export type UndeployedCallData = Anonymize<I43h535ocl8blv> & {
    value: {
        type: string;
    };
};
export type UndeployedWhitelistEntry = PalletKey | ApiKey<IRuntimeCalls> | `query.${NestedKey<PalletsTypedef['__storage']>}` | `tx.${NestedKey<PalletsTypedef['__tx']>}` | `event.${NestedKey<PalletsTypedef['__event']>}` | `error.${NestedKey<PalletsTypedef['__error']>}` | `const.${NestedKey<PalletsTypedef['__const']>}` | `view.${NestedKey<PalletsTypedef['__view']>}`;
type PalletKey = `*.${keyof (IStorage & ICalls & IEvent & IError & IConstants & IRuntimeCalls & IViewFns)}`;
type NestedKey<D extends Record<string, Record<string, any>>> = "*" | {
    [P in keyof D & string]: `${P}.*` | {
        [N in keyof D[P] & string]: `${P}.${N}`;
    }[keyof D[P] & string];
}[keyof D & string];
type ApiKey<D extends Record<string, Record<string, any>>> = "api.*" | {
    [P in keyof D & string]: `api.${P}.*` | {
        [N in keyof D[P] & string]: `api.${P}.${N}`;
    }[keyof D[P] & string];
}[keyof D & string];
