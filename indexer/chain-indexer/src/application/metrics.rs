// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
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

use crate::domain::{Block, ContractAction, Transaction};
use indexer_common::domain::ContractAttributes;
use metrics::{Counter, Gauge, Histogram, counter, gauge, histogram};
use std::time::Duration;

pub struct Metrics {
    block_height: Counter,
    node_block_height: Counter,
    caught_up: Gauge,
    transaction_count: Counter,
    contract_deploy_count: Counter,
    contract_call_count: Counter,
    contract_update_count: Counter,
    gc_run_count: Counter,
    gc_culled_node_count: Counter,
    gc_duration_seconds: Histogram,
}

impl Metrics {
    pub fn new(
        block_height: Option<u64>,
        transaction_count: u64,
        (contract_deploy_count, contract_call_count, contract_update_count): (u64, u64, u64),
    ) -> Self {
        let metrics = Self {
            block_height: counter!("indexer_block_height"),
            node_block_height: counter!("indexer_node_block_height"),
            caught_up: gauge!("indexer_caught_up"),
            transaction_count: counter!("indexer_transaction_count"),
            contract_deploy_count: counter!("indexer_contract_deploy_count"),
            contract_call_count: counter!("indexer_contract_call_count"),
            contract_update_count: counter!("indexer_contract_update_count"),
            gc_run_count: counter!("indexer_gc_run_count"),
            gc_culled_node_count: counter!("indexer_gc_culled_node_count"),
            gc_duration_seconds: histogram!("indexer_gc_duration_seconds"),
        };

        if let Some(block_height) = block_height {
            metrics.block_height.absolute(block_height);
        }
        metrics.transaction_count.absolute(transaction_count);
        metrics
            .contract_deploy_count
            .absolute(contract_deploy_count);
        metrics.contract_call_count.absolute(contract_call_count);
        metrics
            .contract_update_count
            .absolute(contract_update_count);

        metrics
    }

    pub fn update(
        &self,
        block: &Block,
        transactions: &[Transaction],
        node_block_height: u64,
        caught_up: bool,
    ) {
        self.block_height.absolute(block.height);

        self.node_block_height.absolute(node_block_height);

        self.caught_up.set(f64::from(caught_up));

        self.transaction_count.increment(transactions.len() as u64);

        self.contract_call_count.increment(
            transactions
                .iter()
                .filter_map(|t| match t {
                    Transaction::Regular(t) => Some(t),
                    Transaction::System(_) => None,
                })
                .flat_map(|t| {
                    t.contract_actions.iter().filter(|a| {
                        matches!(
                            a,
                            ContractAction {
                                attributes: ContractAttributes::Call { .. },
                                ..
                            }
                        )
                    })
                })
                .count() as u64,
        );

        self.contract_deploy_count.increment(
            transactions
                .iter()
                .filter_map(|t| match t {
                    Transaction::Regular(t) => Some(t),
                    Transaction::System(_) => None,
                })
                .flat_map(|t| {
                    t.contract_actions.iter().filter(|a| {
                        matches!(
                            a,
                            ContractAction {
                                attributes: ContractAttributes::Deploy,
                                ..
                            }
                        )
                    })
                })
                .count() as u64,
        );

        self.contract_update_count.increment(
            transactions
                .iter()
                .filter_map(|t| match t {
                    Transaction::Regular(t) => Some(t),
                    Transaction::System(_) => None,
                })
                .flat_map(|t| {
                    t.contract_actions.iter().filter(|a| {
                        matches!(
                            a,
                            ContractAction {
                                attributes: ContractAttributes::Update,
                                ..
                            }
                        )
                    })
                })
                .count() as u64,
        );
    }

    /// Record one storage-core gc-v1 mark-and-sweep pass.
    pub fn record_gc(&self, duration: Duration, nodes_culled: usize) {
        self.gc_run_count.increment(1);
        self.gc_culled_node_count.increment(nodes_culled as u64);
        self.gc_duration_seconds.record(duration.as_secs_f64());
    }
}
