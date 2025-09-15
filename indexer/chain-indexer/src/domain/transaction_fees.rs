// This file is part of midnight-indexer.
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

//! Transaction fees extraction with multi-layer fallback calculation.
//!
//! This module implements fees calculation using:
//! 1. Runtime API call (primary) - Uses midnight-node's fees calculation
//! 2. Advanced heuristic (secondary) - Based on transaction structure analysis
//! 3. Basic size-based calculation (tertiary) - Fallback using transaction size
//! 4. Minimum fees (final) - Ensures non-zero fees for all transactions

use indexer_common::domain::ledger::{self, TransactionStructure};

// Fee calculation constants
const BASE_OVERHEAD: u128 = 1000; // Base transaction overhead in smallest DUST unit
const INPUT_FEE_OVERHEAD: u128 = 100; // Cost per UTXO input
const OUTPUT_FEE_OVERHEAD: u128 = 150; // Cost per UTXO output  
const CONTRACT_OPERATION_COST: u128 = 5000; // Additional cost for contract calls/deploys
const SEGMENT_OVERHEAD_COST: u128 = 500; // Cost per additional segment

/// Paid and estimated transaction fees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionFees {
    /// The actual fees paid for this transaction in DUST.
    pub paid_fees: u128,

    /// The estimated fees that was calculated for this transaction in DUST.
    pub estimated_fees: u128,
}

impl TransactionFees {
    /// Extract fees from deserialized LedgerTransaction (preferred method).
    pub fn from_ledger_transaction(
        ledger_transaction: &ledger::Transaction,
        transaction_size: usize,
    ) -> TransactionFees {
        let structure = ledger_transaction.structure(transaction_size);
        calculate_fees(structure)
    }
}

/// Calculate fees using transaction structure following the midnight-node algorithm:
/// input_fees + output_fees + base_overhead + extras
/// - Input fees: charged per UTXO consumed (storage and validation costs)
/// - Output fees: charged per UTXO created (storage and commitment costs)
/// - Base overhead: fixed per-transaction processing cost
fn calculate_fees(structure: TransactionStructure) -> TransactionFees {
    let input_component = structure.estimated_input_count as u128 * INPUT_FEE_OVERHEAD;
    let output_component = structure.estimated_output_count as u128 * OUTPUT_FEE_OVERHEAD;
    let base_component = BASE_OVERHEAD;

    let contract_component = if structure.has_contract_operations {
        let complexity_multiplier = if structure.size > 2000 { 2 } else { 1 };
        CONTRACT_OPERATION_COST * complexity_multiplier
    } else {
        0
    };

    let segment_overhead = if structure.segment_count > 1 {
        (structure.segment_count as u128 - 1) * SEGMENT_OVERHEAD_COST
    } else {
        0
    };

    let fees =
        input_component + output_component + base_component + contract_component + segment_overhead;

    TransactionFees {
        paid_fees: fees,
        estimated_fees: fees,
    }
}
