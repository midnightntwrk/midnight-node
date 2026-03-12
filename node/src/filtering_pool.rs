// This file is part of midnight-node.
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

use async_trait::async_trait;
use midnight_node_ledger::types::{Op, Tx};
use pallet_midnight::MidnightRuntimeApi;
use parity_scale_codec::{Decode, Encode};
use prometheus_endpoint::{Counter, CounterVec, Opts, Registry, U64, register};
use sc_transaction_pool::{ChainApi, FullChainApi, TransactionPoolWrapper};
use sc_transaction_pool_api::error::Error as TxPoolError;
use sc_transaction_pool_api::{
	ChainEvent, ImportNotificationStream, LocalTransactionFor, LocalTransactionPool,
	MaintainedTransactionPool, PoolStatus, ReadyTransactions, TransactionFor, TransactionSource,
	TransactionStatusStreamFor, TxHash, TxInvalidityReportMap,
};
use sp_runtime::traits::Block as BlockT;
use std::{collections::HashMap, pin::Pin, sync::Arc};

const LOG_TARGET: &str = "filtering_pool";

#[derive(Debug, Clone, Copy, Default)]
pub struct TxFilterConfig {
	pub enabled: bool,
	pub deny_deploy: bool,
	pub deny_maintain: bool,
}

impl TxFilterConfig {
	pub fn enabled() -> Self {
		Self { enabled: true, deny_deploy: true, deny_maintain: true }
	}

	pub fn disabled() -> Self {
		Self { enabled: false, deny_deploy: false, deny_maintain: false }
	}
}

pub struct FilteringTransactionPool<Block, Client>
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ 'static,
	Client::Api:
		sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> + MidnightRuntimeApi<Block>,
{
	/// Is filtering behavior enabled. If false it behaves like 'inner' pool.
	enabled: bool,
	inner: TransactionPoolWrapper<Block, Client>,
	client: Arc<Client>,
	metrics: FilteringMetrics,
	deny_deploy: bool,
	deny_maintain: bool,
}

impl<Block, Client> FilteringTransactionPool<Block, Client>
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ 'static,
	Client::Api:
		sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> + MidnightRuntimeApi<Block>,
{
	pub(crate) fn new(
		config: TxFilterConfig,
		inner: TransactionPoolWrapper<Block, Client>,
		client: Arc<Client>,
		metrics: FilteringMetrics,
	) -> Self {
		Self {
			enabled: config.enabled,
			inner,
			client,
			metrics,
			deny_deploy: config.deny_deploy,
			deny_maintain: config.deny_maintain,
		}
	}

	fn is_forbidden_tx(&self, tx: &Tx) -> bool {
		tx.operations.iter().any(|operation| match operation {
			Op::Deploy { .. } => {
				if self.deny_deploy {
					self.metrics.deny(DEPLOY);
					true
				} else {
					false
				}
			},
			Op::Maintain { .. } => {
				if self.deny_maintain {
					self.metrics.deny(MAINTAIN);
					true
				} else {
					false
				}
			},
			Op::Call { .. } | Op::ClaimRewards { .. } => false,
		})
	}

	fn should_accept_extrinsic(
		&self,
		at: <Block as BlockT>::Hash,
		xt: &<Block as BlockT>::Extrinsic,
	) -> bool {
		if !self.enabled {
			return true;
		}
		self.metrics.receive();
		let Ok(decoded_xt) =
			midnight_node_runtime::UncheckedExtrinsic::decode(&mut &xt.encode()[..])
		else {
			log::warn!(
				target: LOG_TARGET,
				"⚠️ Not denying transaction that failed to decode as runtime UncheckedExtrinsic",
			);
			self.metrics.forward();
			return true;
		};

		if let midnight_node_runtime::RuntimeCall::Midnight(
			midnight_node_runtime::MidnightCall::send_mn_transaction { midnight_tx },
		) = decoded_xt.function
		{
			match self.client.runtime_api().get_decoded_transaction(at, midnight_tx) {
				Ok(decoded_tx_result) => match decoded_tx_result {
					Ok(decoded_tx) => {
						if self.is_forbidden_tx(&decoded_tx) {
							log::info!(
								target: LOG_TARGET,
								"🚫 Blocking midnight transaction based on filtering policy",
							);
							return false;
						}
					},
					Err(error) => {
						self.metrics.deny(PARSE_ERROR);
						log::error!(
							target: LOG_TARGET,
							"⚠️ Unable to decode midnight transaction, dropping it: {:?}",
							error
						);
						return false;
					},
				},
				Err(error) => {
					self.metrics.deny(PARSE_ERROR);
					log::error!(
						target: LOG_TARGET,
						"❌ Runtime API call get_decoded_transaction failed: {:?}",
						error
					);
					return false;
				},
			}
		}
		self.metrics.forward();
		true
	}
}

#[async_trait]
impl<Block, Client> sc_transaction_pool_api::TransactionPool
	for FilteringTransactionPool<Block, Client>
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ 'static,
	Client::Api:
		sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> + MidnightRuntimeApi<Block>,
{
	type Block = Block;
	type Hash = <<FullChainApi<Client, Block> as ChainApi>::Block as BlockT>::Hash;
	type InPoolTransaction =
		<TransactionPoolWrapper<Block, Client> as sc_service::TransactionPool>::InPoolTransaction;
	type Error = <FullChainApi<Client, Block> as ChainApi>::Error;

	async fn submit_at(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xts: Vec<TransactionFor<Self>>,
	) -> Result<Vec<Result<TxHash<Self>, Self::Error>>, Self::Error> {
		let total_xts = xts.len();
		let mut filtered = Vec::with_capacity(total_xts);
		for xt in xts {
			if self.should_accept_extrinsic(at, &xt) {
				filtered.push(xt);
			}
		}
		self.inner.submit_at(at, source, filtered).await
	}

	async fn submit_one(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> Result<TxHash<Self>, Self::Error> {
		if !self.should_accept_extrinsic(at, &xt) {
			return Err(TxPoolError::ImmediatelyDropped.into());
		}

		self.inner.submit_one(at, source, xt).await
	}

	async fn submit_and_watch(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> Result<Pin<Box<TransactionStatusStreamFor<Self>>>, Self::Error> {
		if !self.should_accept_extrinsic(at, &xt) {
			return Err(TxPoolError::ImmediatelyDropped.into());
		}

		self.inner.submit_and_watch(at, source, xt).await
	}

	async fn ready_at(
		&self,
		at: <Self::Block as BlockT>::Hash,
	) -> Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send> {
		self.inner.ready_at(at).await
	}

	fn ready(&self) -> Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send> {
		self.inner.ready()
	}

	async fn report_invalid(
		&self,
		at: Option<<Self::Block as BlockT>::Hash>,
		invalid_tx_errors: TxInvalidityReportMap<TxHash<Self>>,
	) -> Vec<Arc<Self::InPoolTransaction>> {
		self.inner.report_invalid(at, invalid_tx_errors).await
	}

	fn futures(&self) -> Vec<Self::InPoolTransaction> {
		self.inner.futures()
	}

	fn status(&self) -> PoolStatus {
		self.inner.status()
	}

	fn import_notification_stream(&self) -> ImportNotificationStream<TxHash<Self>> {
		self.inner.import_notification_stream()
	}

	fn on_broadcasted(&self, propagations: HashMap<TxHash<Self>, Vec<String>>) {
		self.inner.on_broadcasted(propagations)
	}

	fn hash_of(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
		self.inner.hash_of(xt)
	}

	fn ready_transaction(&self, hash: &TxHash<Self>) -> Option<Arc<Self::InPoolTransaction>> {
		self.inner.ready_transaction(hash)
	}

	async fn ready_at_with_timeout(
		&self,
		at: <Self::Block as BlockT>::Hash,
		timeout: std::time::Duration,
	) -> Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send> {
		self.inner.ready_at_with_timeout(at, timeout).await
	}
}

#[async_trait]
impl<Block, Client> MaintainedTransactionPool for FilteringTransactionPool<Block, Client>
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ 'static,
	Client::Api:
		sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> + MidnightRuntimeApi<Block>,
{
	async fn maintain(&self, event: ChainEvent<Self::Block>) {
		self.inner.maintain(event).await
	}
}

impl<Block, Client> LocalTransactionPool for FilteringTransactionPool<Block, Client>
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ 'static,
	Client::Api:
		sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> + MidnightRuntimeApi<Block>,
{
	type Block = Block;
	type Hash = TxHash<Self>;
	type Error = <FullChainApi<Client, Block> as ChainApi>::Error;

	fn submit_local(
		&self,
		at: <Self::Block as BlockT>::Hash,
		xt: LocalTransactionFor<Self>,
	) -> Result<Self::Hash, Self::Error> {
		if !self.should_accept_extrinsic(at, &xt) {
			return Err(TxPoolError::ImmediatelyDropped.into());
		}
		self.inner.submit_local(at, xt)
	}
}

const DEPLOY: &str = "deploy";
const MAINTAIN: &str = "maintain";
const PARSE_ERROR: &str = "parse_error";

const LABELS: &[&str] = &[DEPLOY, MAINTAIN, PARSE_ERROR];

pub struct FilteringMetrics {
	forwarded_count: Option<Counter<U64>>,
	received_count: Option<Counter<U64>>,
	denied_count: Option<CounterVec<U64>>,
}

impl FilteringMetrics {
	pub fn new(registry: Option<&Registry>) -> Self {
		let forwarded_count = registry.map(|r| {
			register(
				Counter::new(
					"gateway_tx_forwarded_total",
					"Transactions forwarded by filtering gateway node",
				)
				.unwrap(),
				r,
			)
			.unwrap()
		});
		let received_count = registry.map(|r| {
			register(
				Counter::new(
					"gateway_tx_received_total",
					"Transactions received by filtering gateway node",
				)
				.unwrap(),
				r,
			)
			.unwrap()
		});
		let denied_count = {
			let opts = Opts::new(
				"gateway_tx_denied_total",
				"Transactions denied by filtering gateway node",
			);
			registry.map(|r| register(CounterVec::new(opts, LABELS).unwrap(), r).unwrap())
		};
		Self { forwarded_count, received_count, denied_count }
	}

	fn deny(&self, reason: &str) {
		if let Some(c) = &self.denied_count {
			let _ = c.get_metric_with_label_values(&[reason]).map(|m| m.inc());
		}
	}

	fn forward(&self) {
		if let Some(c) = &self.forwarded_count {
			c.inc();
		}
	}

	fn receive(&self) {
		if let Some(c) = &self.received_count {
			c.inc();
		}
	}
}
