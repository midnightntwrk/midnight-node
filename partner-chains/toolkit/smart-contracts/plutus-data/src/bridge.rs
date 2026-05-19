//! Constants and types used by the token bridge

use cardano_serialization_lib::{JsError, MetadataList, TransactionMetadatum};

/// Arbitrary key, used as top-level metadatum key 6500973 = 0x63326d ~= 'c2n'
pub const TOKEN_TRANSFER_METADATUM_KEY: u64 = 6500973;

/// Metadata item for transfer to specified address is a list with this address encoded as bytes.
pub fn transfer_to_addressed_transaction_metadatum(
	address_bytes: &[u8],
) -> Result<TransactionMetadatum, JsError> {
	let mut list = MetadataList::new();
	list.add(&TransactionMetadatum::new_bytes(address_bytes.to_vec())?);
	Ok(TransactionMetadatum::new_list(&list))
}

/// Metadata of reserve transfer is an empty list.
pub fn transfer_to_reserve_metadatum() -> TransactionMetadatum {
	TransactionMetadatum::new_list(&MetadataList::new())
}
