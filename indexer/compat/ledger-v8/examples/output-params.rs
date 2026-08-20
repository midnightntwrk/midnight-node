// This file is part of midnight-ledger.
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

//! Outputs the current parameters as a serialized file, ready for constructing a parameter update
//! transaction with the toolkit. This file serves a practical purpose to update the parameters,
//! and as a paper-trail for why an update was performed.

use std::path::PathBuf;

use base_crypto::{
    cost_model::{CostDuration, SyntheticCost},
    time::Duration,
};
use midnight_ledger::structure::{
    INITIAL_LIMITS, INITIAL_PARAMETERS, INITIAL_TRANSACTION_COST_MODEL, LedgerParameters,
    TransactionCostModel, TransactionLimits,
};
use onchain_vm::cost_model::{CostModel, INITIAL_COST_MODEL};
use serialize::tagged_serialize;

// As output by generate-cost-model on our benchmark machine; ran against ledger-7.0.0-rc.1
const UPDATED_COST_MODEL: CostModel = CostModel {
    read_time_batched_4k: INITIAL_COST_MODEL.read_time_batched_4k,
    read_time_synchronous_4k: INITIAL_COST_MODEL.read_time_synchronous_4k,
    pop: CostDuration::from_picoseconds(1655719),
    pushs_cell: CostDuration::from_picoseconds(440148),
    pushs_bmt: CostDuration::from_picoseconds(410602),
    pushs_array: CostDuration::from_picoseconds(2425886),
    pushs_null: CostDuration::from_picoseconds(262204),
    pushs_map: CostDuration::from_picoseconds(2807319),
    lt: CostDuration::from_picoseconds(2294435),
    add: CostDuration::from_picoseconds(2446112),
    verifier_key_load: CostDuration::from_picoseconds(1529923104),
    ckpt: CostDuration::from_picoseconds(102363),
    signature_verify_constant: CostDuration::from_picoseconds(97304512),
    signature_verify_coeff_size: CostDuration::from_picoseconds(4725),
    gc_rcmap_constant: CostDuration::from_picoseconds(355056516),
    gc_rcmap_coeff_keys_removed_size: CostDuration::from_picoseconds(228409893),
    new_bmt: CostDuration::from_picoseconds(1906647),
    new_cell: CostDuration::from_picoseconds(1721622),
    new_null: CostDuration::from_picoseconds(788421),
    new_array: CostDuration::from_picoseconds(2909716),
    new_map: CostDuration::from_picoseconds(2907810),
    and: CostDuration::from_picoseconds(2361629),
    rem_bmt_constant: CostDuration::from_picoseconds(0),
    rem_bmt_coeff_container_log_size: CostDuration::from_picoseconds(98282212),
    rem_bmt_coeff_key_size: CostDuration::from_picoseconds(14358034),
    rem_map_constant: CostDuration::from_picoseconds(9163731),
    rem_map_coeff_key_size: CostDuration::from_picoseconds(12306),
    rem_map_coeff_container_log_size: CostDuration::from_picoseconds(3097756),
    noop_constant: CostDuration::from_picoseconds(103089),
    noop_coeff_arg: CostDuration::from_picoseconds(0),
    subi: CostDuration::from_picoseconds(1957428),
    popeq_constant: CostDuration::from_picoseconds(693593),
    popeq_coeff_value_size: CostDuration::from_picoseconds(389),
    remc_map_constant: CostDuration::from_picoseconds(8927319),
    remc_map_coeff_key_size: CostDuration::from_picoseconds(12372),
    remc_map_coeff_container_log_size: CostDuration::from_picoseconds(3100736),
    remc_bmt_constant: CostDuration::from_picoseconds(0),
    remc_bmt_coeff_container_log_size: CostDuration::from_picoseconds(97600528),
    remc_bmt_coeff_key_size: CostDuration::from_picoseconds(13523068),
    dup_constant: CostDuration::from_picoseconds(3776962),
    dup_coeff_arg: CostDuration::from_picoseconds(1570180),
    transient_hash: CostDuration::from_picoseconds(86465888),
    ec_add: CostDuration::from_picoseconds(376004),
    type_null: CostDuration::from_picoseconds(1588063),
    type_bmt: CostDuration::from_picoseconds(1879039),
    type_map: CostDuration::from_picoseconds(1883171),
    type_cell: CostDuration::from_picoseconds(1749395),
    type_array: CostDuration::from_picoseconds(2045565),
    neg: CostDuration::from_picoseconds(1889123),
    size_array: CostDuration::from_picoseconds(3854833),
    size_map: CostDuration::from_picoseconds(4346344),
    size_bmt: CostDuration::from_picoseconds(1691797),
    addi: CostDuration::from_picoseconds(1961762),
    push_cell: CostDuration::from_picoseconds(438751),
    push_bmt: CostDuration::from_picoseconds(409282),
    push_array: CostDuration::from_picoseconds(2416533),
    push_null: CostDuration::from_picoseconds(263938),
    push_map: CostDuration::from_picoseconds(2802478),
    concat_constant: CostDuration::from_picoseconds(1938114),
    concat_coeff_total_size: CostDuration::from_picoseconds(7034),
    idxp_array: CostDuration::from_picoseconds(12782105),
    idxp_bmt_constant: CostDuration::from_picoseconds(9559423),
    idxp_bmt_coeff_key_size: CostDuration::from_picoseconds(15702),
    idxp_bmt_coeff_container_log_size: CostDuration::from_picoseconds(2299),
    idxp_map_constant: CostDuration::from_picoseconds(14521179),
    idxp_map_coeff_key_size: CostDuration::from_picoseconds(16033),
    idxp_map_coeff_container_log_size: CostDuration::from_picoseconds(0),
    member_constant: CostDuration::from_picoseconds(6313602),
    member_coeff_key_size: CostDuration::from_picoseconds(6439),
    member_coeff_container_log_size: CostDuration::from_picoseconds(0),
    insc_map_constant: CostDuration::from_picoseconds(26836597),
    insc_map_coeff_key_size: CostDuration::from_picoseconds(24762),
    insc_map_coeff_container_log_size: CostDuration::from_picoseconds(5294734),
    insc_array: CostDuration::from_picoseconds(25876639),
    insc_bmt_constant: CostDuration::from_picoseconds(0),
    insc_bmt_coeff_key_size: CostDuration::from_picoseconds(9993440),
    insc_bmt_coeff_container_log_size: CostDuration::from_picoseconds(101984546),
    pedersen_valid: CostDuration::from_picoseconds(277513481),
    hash_to_curve: CostDuration::from_picoseconds(338977834),
    proof_verify_constant: CostDuration::from_picoseconds(3273586253),
    proof_verify_coeff_size: CostDuration::from_picoseconds(4555132),
    or: CostDuration::from_picoseconds(2322338),
    branch_constant: CostDuration::from_picoseconds(637846),
    branch_coeff_arg: CostDuration::from_picoseconds(7),
    idxpc_array: CostDuration::from_picoseconds(12703071),
    idxpc_map_constant: CostDuration::from_picoseconds(14391349),
    idxpc_map_coeff_key_size: CostDuration::from_picoseconds(16162),
    idxpc_map_coeff_container_log_size: CostDuration::from_picoseconds(0),
    idxpc_bmt_constant: CostDuration::from_picoseconds(8911032),
    idxpc_bmt_coeff_key_size: CostDuration::from_picoseconds(4903),
    idxpc_bmt_coeff_container_log_size: CostDuration::from_picoseconds(4239),
    eq: CostDuration::from_picoseconds(2283612),
    ec_mul: CostDuration::from_picoseconds(127815559),
    swap_constant: CostDuration::from_picoseconds(2658852),
    swap_coeff_arg: CostDuration::from_picoseconds(1505645),
    sub: CostDuration::from_picoseconds(2297629),
    update_rcmap_constant: CostDuration::from_picoseconds(739218490),
    update_rcmap_coeff_keys_added_size: CostDuration::from_picoseconds(149190766),
    get_writes_constant: CostDuration::from_picoseconds(0),
    get_writes_coeff_keys_added_size: CostDuration::from_picoseconds(10244381),
    jmp_constant: CostDuration::from_picoseconds(103256),
    jmp_coeff_arg: CostDuration::from_picoseconds(0),
    ins_array: CostDuration::from_picoseconds(25107400),
    ins_map_constant: CostDuration::from_picoseconds(43195449),
    ins_map_coeff_container_log_size: CostDuration::from_picoseconds(4525747),
    ins_map_coeff_key_size: CostDuration::from_picoseconds(42051),
    ins_bmt_constant: CostDuration::from_picoseconds(0),
    ins_bmt_coeff_key_size: CostDuration::from_picoseconds(14803308),
    ins_bmt_coeff_container_log_size: CostDuration::from_picoseconds(96985698),
    popeqc_constant: CostDuration::from_picoseconds(700503),
    popeqc_coeff_value_size: CostDuration::from_picoseconds(387),
    idxc_bmt_constant: CostDuration::from_picoseconds(8298887),
    idxc_bmt_coeff_container_log_size: CostDuration::from_picoseconds(2066),
    idxc_bmt_coeff_key_size: CostDuration::from_picoseconds(20906),
    idxc_array: CostDuration::from_picoseconds(10035943),
    idxc_map_constant: CostDuration::from_picoseconds(10819711),
    idxc_map_coeff_container_log_size: CostDuration::from_picoseconds(4748),
    idxc_map_coeff_key_size: CostDuration::from_picoseconds(17761),
    root: CostDuration::from_picoseconds(1825972),
    concatc_constant: CostDuration::from_picoseconds(2762557),
    concatc_coeff_total_size: CostDuration::from_picoseconds(6533),
    idx_map_constant: CostDuration::from_picoseconds(10899801),
    idx_map_coeff_container_log_size: CostDuration::from_picoseconds(26838),
    idx_map_coeff_key_size: CostDuration::from_picoseconds(18058),
    idx_array: CostDuration::from_picoseconds(9993652),
    idx_bmt_constant: CostDuration::from_picoseconds(8980653),
    idx_bmt_coeff_key_size: CostDuration::from_picoseconds(0),
    idx_bmt_coeff_container_log_size: CostDuration::from_picoseconds(6893),
    log_map_constant: CostDuration::from_picoseconds(738588159),
    log_map_coeff_value_size: CostDuration::from_picoseconds(71340),
    log_bmt_constant: CostDuration::from_picoseconds(0),
    log_bmt_coeff_value_size: CostDuration::from_picoseconds(102149),
    log_cell_constant: CostDuration::from_picoseconds(3054417),
    log_cell_coeff_value_size: CostDuration::from_picoseconds(505),
    log_null_constant: CostDuration::from_picoseconds(2163252),
    log_null_coeff_value_size: CostDuration::from_picoseconds(0),
    log_array_constant: CostDuration::from_picoseconds(0),
    log_array_coeff_value_size: CostDuration::from_picoseconds(1315830),
};
const UPDATED_PARAMS: LedgerParameters = LedgerParameters {
    cost_model: TransactionCostModel {
        runtime_cost_model: UPDATED_COST_MODEL,
        ..INITIAL_TRANSACTION_COST_MODEL
    },
    limits: TransactionLimits {
        block_limits: SyntheticCost {
            read_time: CostDuration::from_picoseconds(2_000_000_000_000),
            compute_time: CostDuration::from_picoseconds(2_000_000_000_000),
            block_usage: 1_000_000,
            bytes_written: 50_000,
            bytes_churned: 50_000_000,
        },
        ..INITIAL_LIMITS
    },
    global_ttl: Duration::from_hours(336),
    ..INITIAL_PARAMETERS
};

fn main() {
    let Some(fname) = std::env::args_os().nth(1) else {
        eprintln!("provide parameter name to output");
        return;
    };
    let mut path = PathBuf::new();
    path.push("params");
    path.push(fname);
    let f_binary = std::fs::File::create(path.with_added_extension("bin")).unwrap();
    let mut f_json = std::fs::File::create(path.with_added_extension("json")).unwrap();
    tagged_serialize(&UPDATED_PARAMS, f_binary).unwrap();
    serde_json::to_writer_pretty(&mut f_json, &UPDATED_PARAMS).unwrap();
}
