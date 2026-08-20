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

use crate::domain::{ByteArray, ByteVec, UnshieldedAddress};
use serde::{Deserialize, Serialize};
use sqlx::Type;
use thiserror::Error;

/// Cardano main-chain transaction hash for correlation with the Partner Chain.
pub type McTxHash = ByteArray<32>;

/// Hash of the Midnight system transaction produced by a bridge handler.
pub type MidnightTxHash = ByteArray<32>;

/// Maximum length in bytes of a Midnight recipient encoded in the bridge metadata.
pub const BRIDGE_RECIPIENT_MAX_BYTES: usize = 32;

/// Recipient address parsed from Cardano bridge metadata, bounded to 32 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(transparent)]
pub struct BridgeRecipient(ByteVec);

impl BridgeRecipient {
    pub fn new(bytes: Vec<u8>) -> Result<Self, BridgeRecipientError> {
        if bytes.len() > BRIDGE_RECIPIENT_MAX_BYTES {
            return Err(BridgeRecipientError::TooLong {
                max: BRIDGE_RECIPIENT_MAX_BYTES,
                actual: bytes.len(),
            });
        }
        Ok(Self(ByteVec(bytes)))
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BridgeRecipientError {
    #[error("bridge recipient is {actual} bytes, exceeds max of {max}")]
    TooLong { max: usize, actual: usize },
}

/// Discriminator for the c2m-bridge event variants.
///
/// Persisted as a Postgres ENUM (`PROTOCOL_BRIDGE_EVENT_VARIANT`) and as a SQLite TEXT CHECK column
/// to enable indexed filtering by variant without unpacking the per-variant payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[sqlx(type_name = "PROTOCOL_BRIDGE_EVENT_VARIANT", rename_all = "PascalCase")]
pub enum BridgeEventVariant {
    UserTransfer,
    ReserveTransfer,
    InvalidTransfer,
    UnapprovedTransfer,
    SubminimalFlushTransfer,
}

/// Events emitted by the c2m-bridge pallet (`pallet-c2m-bridge`, runtime pallet index 33).
///
/// Each variant corresponds to a distinct outcome of an observed Cardano-to-Midnight bridge
/// transaction. See the c2m-bridge pallet's flowchart in the bridge protocol docs for the
/// decision flow that produces each variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeEvent {
    /// Approved user deposit. Funds credited to `recipient` via DistributeNight CardanoBridge.
    UserTransfer {
        mc_tx_hash: McTxHash,
        amount: u64,
        recipient: BridgeRecipient,
        midnight_tx_hash: MidnightTxHash,
    },

    /// Reserve top-up. Funds credited to the protocol Reserve pool via DistributeReserve.
    /// Bypasses the approval gate (Reserve is protocol-owned, see Andrzej's framing 29 Apr 2026).
    ReserveTransfer {
        mc_tx_hash: McTxHash,
        amount: u64,
        midnight_tx_hash: MidnightTxHash,
    },

    /// Malformed bridge metadata. Funds redirected to treasury.
    InvalidTransfer {
        mc_tx_hash: McTxHash,
        amount: u64,
        midnight_tx_hash: MidnightTxHash,
    },

    /// User deposit whose Cardano tx hash was not in `ApprovedTransactions` at observation time.
    /// Funds redirected to treasury. The `recipient` is the would-be recipient parsed from
    /// metadata.
    UnapprovedTransfer {
        mc_tx_hash: McTxHash,
        amount: u64,
        recipient: BridgeRecipient,
        midnight_tx_hash: MidnightTxHash,
    },

    /// Aggregated subminimum transfers flushed to treasury when the accumulator threshold is
    /// reached. `count` is the number of subminimum Cardano txs that contributed to this flush.
    ///
    /// Deliberately carries no `mc_tx_hash`: the flush covers value from multiple Cardano
    /// transactions, so any single hash (including the one that crossed the threshold) would be
    /// a misleading correlator. Agreed in the 29 Apr 2026 schema convergence and reconfirmed by
    /// the architect on 12 Jun 2026.
    SubminimalFlushTransfer {
        amount: u64,
        count: u32,
        midnight_tx_hash: MidnightTxHash,
    },
}

impl BridgeEvent {
    pub fn variant(&self) -> BridgeEventVariant {
        match self {
            Self::UserTransfer { .. } => BridgeEventVariant::UserTransfer,
            Self::ReserveTransfer { .. } => BridgeEventVariant::ReserveTransfer,
            Self::InvalidTransfer { .. } => BridgeEventVariant::InvalidTransfer,
            Self::UnapprovedTransfer { .. } => BridgeEventVariant::UnapprovedTransfer,
            Self::SubminimalFlushTransfer { .. } => BridgeEventVariant::SubminimalFlushTransfer,
        }
    }

    /// Returns the Cardano main-chain tx hash, or None for `SubminimalFlushTransfer`
    /// (an aggregate spanning multiple Cardano txs).
    pub fn mc_tx_hash(&self) -> Option<&McTxHash> {
        match self {
            Self::UserTransfer { mc_tx_hash, .. }
            | Self::ReserveTransfer { mc_tx_hash, .. }
            | Self::InvalidTransfer { mc_tx_hash, .. }
            | Self::UnapprovedTransfer { mc_tx_hash, .. } => Some(mc_tx_hash),
            Self::SubminimalFlushTransfer { .. } => None,
        }
    }

    /// Returns the recipient address for variants that carry one (UserTransfer,
    /// UnapprovedTransfer).
    pub fn recipient(&self) -> Option<&BridgeRecipient> {
        match self {
            Self::UserTransfer { recipient, .. } | Self::UnapprovedTransfer { recipient, .. } => {
                Some(recipient)
            }
            _ => None,
        }
    }

    pub fn amount(&self) -> u64 {
        match self {
            Self::UserTransfer { amount, .. }
            | Self::ReserveTransfer { amount, .. }
            | Self::InvalidTransfer { amount, .. }
            | Self::UnapprovedTransfer { amount, .. }
            | Self::SubminimalFlushTransfer { amount, .. } => *amount,
        }
    }

    pub fn midnight_tx_hash(&self) -> &MidnightTxHash {
        match self {
            Self::UserTransfer {
                midnight_tx_hash, ..
            }
            | Self::ReserveTransfer {
                midnight_tx_hash, ..
            }
            | Self::InvalidTransfer {
                midnight_tx_hash, ..
            }
            | Self::UnapprovedTransfer {
                midnight_tx_hash, ..
            }
            | Self::SubminimalFlushTransfer {
                midnight_tx_hash, ..
            } => midnight_tx_hash,
        }
    }
}

/// A claim of bridged NIGHT, parsed from a regular `ClaimRewardsTransaction` with
/// `ClaimKind::CardanoBridge`.
///
/// Extracted in the ledger-9 apply path (`LedgerState::apply_regular_transaction`): the `kind`
/// discriminator is read from the deserialized claim, the `recipient` is the claim owner's
/// address, and the `amount` is the claim value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeClaim {
    pub recipient: UnshieldedAddress,
    pub amount: u128,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_recipient_accepts_up_to_max() {
        let bytes = vec![1u8; BRIDGE_RECIPIENT_MAX_BYTES];
        let recipient = BridgeRecipient::new(bytes.clone()).unwrap();
        assert_eq!(recipient.as_bytes(), bytes.as_slice());
    }

    #[test]
    fn bridge_recipient_rejects_too_long() {
        let bytes = vec![1u8; BRIDGE_RECIPIENT_MAX_BYTES + 1];
        let err = BridgeRecipient::new(bytes).unwrap_err();
        assert_eq!(
            err,
            BridgeRecipientError::TooLong {
                max: BRIDGE_RECIPIENT_MAX_BYTES,
                actual: BRIDGE_RECIPIENT_MAX_BYTES + 1
            }
        );
    }

    #[test]
    fn variant_discriminator_matches() {
        let recipient = BridgeRecipient::new(vec![0u8; 32]).unwrap();
        let mc = ByteArray([1u8; 32]);
        let mn = ByteArray([2u8; 32]);

        assert_eq!(
            BridgeEvent::UserTransfer {
                mc_tx_hash: mc,
                amount: 100,
                recipient: recipient.clone(),
                midnight_tx_hash: mn,
            }
            .variant(),
            BridgeEventVariant::UserTransfer
        );

        assert_eq!(
            BridgeEvent::SubminimalFlushTransfer {
                amount: 999,
                count: 3,
                midnight_tx_hash: mn,
            }
            .variant(),
            BridgeEventVariant::SubminimalFlushTransfer
        );
    }

    #[test]
    fn subminimal_flush_has_no_mc_tx_hash() {
        let mn = ByteArray([2u8; 32]);
        let event = BridgeEvent::SubminimalFlushTransfer {
            amount: 999,
            count: 3,
            midnight_tx_hash: mn,
        };
        assert!(event.mc_tx_hash().is_none());
        assert!(event.recipient().is_none());
    }

    #[test]
    fn json_roundtrip() {
        let recipient = BridgeRecipient::new(vec![0xab; 32]).unwrap();
        let event = BridgeEvent::UserTransfer {
            mc_tx_hash: ByteArray([1u8; 32]),
            amount: 1_000_000,
            recipient,
            midnight_tx_hash: ByteArray([2u8; 32]),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: BridgeEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event);
    }
}
