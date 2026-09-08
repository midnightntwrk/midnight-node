use jsonrpsee::types::error::{ErrorObject, ErrorObjectOwned, INVALID_PARAMS_CODE};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum StateRpcError {
    #[error("Unable to decode contract address: {0}")]
    BadContractAddress(String),
    #[error("Unable to decode account address: {0}")]
    BadAccountAddress(String),
    #[error("Contract not present")]
    ContractNotPresent,
    #[error("Unable to get requested contract state")]
    UnableToGetContractState,
    #[error("Unable to get requested zswap chain state")]
    UnableToGetZSwapChainState,
    #[error("Unable to get requested zswap state root")]
    UnableToGetZSwapStateRoot,
    #[error("Unable to get requested ledger state root")]
    UnableToGetLedgerStateRoot,
}

#[derive(Debug, thiserror::Error)]
pub enum BlockRpcError {
    #[error("Error while getting block: {0}")]
    UnableToGetBlock(String),
    #[error("Unable to get block by hash")]
    BlockNotFound,
    #[error("Unable to get ledger state")]
    UnableToGetLedgerState,
    #[error("Unable to decode transactions for block: {0}")]
    UnableToDecodeTransactions(String),
    #[error("Unable to serialize block to JSON: {0}")]
    UnableToSerializeBlock(String),
    #[error("Unable to read chain name")]
    UnableToGetChainVersion,
}

#[derive(Debug, Serialize, thiserror::Error)]
pub enum EventsError {
    #[error("Unable to hex decode event: {event}, because of {error}")]
    HexDecode { event: String, error: String },
    #[error("Unable to decode event: {event}, because of {error}")]
    Decode { event: String, error: String },
    #[error("Unable to serialize event to json: {event}, because of {error}")]
    UnableToSerializeEvent { event: String, error: String },
}

impl From<StateRpcError> for ErrorObjectOwned {
    fn from(value: StateRpcError) -> Self {
        ErrorObject::owned(INVALID_PARAMS_CODE, value.to_string(), None::<()>)
    }
}

impl From<BlockRpcError> for ErrorObjectOwned {
    fn from(value: BlockRpcError) -> Self {
        ErrorObject::owned(INVALID_PARAMS_CODE, value.to_string(), None::<()>)
    }
}

impl From<EventsError> for ErrorObjectOwned {
    fn from(value: EventsError) -> Self {
        ErrorObject::owned(INVALID_PARAMS_CODE, value.to_string(), None::<()>)
    }
}
