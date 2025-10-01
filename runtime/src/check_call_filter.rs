use super::{RuntimeCall, RuntimeOrigin};
use frame_support::{pallet_prelude::TransactionSource, traits::Contains};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{DispatchInfoOf, TransactionExtension, ValidateResult},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
};

/// Filter that whitelists Governance calls and Sudo
struct GovernanceAuthorityCallFilter;
impl Contains<RuntimeCall> for GovernanceAuthorityCallFilter {
	fn contains(call: &RuntimeCall) -> bool {
		matches!(
			call,
			RuntimeCall::Council(_)
				| RuntimeCall::TechnicalCommittee(_)
				| RuntimeCall::Sudo(_)
				| RuntimeCall::FederatedAuthority(
					pallet_federated_authority::Call::motion_close { .. }
				) | RuntimeCall::System(frame_system::Call::apply_authorized_upgrade { .. })
		)
	}
}

/// Nothing but Governance calls are allowed
type CallFilter = GovernanceAuthorityCallFilter;

/// `TransactionExtension` that enforces the `CallFilter`` rules
#[derive(Encode, Decode, Debug, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo)]
pub struct CheckCallFilter;

impl TransactionExtension<RuntimeCall> for CheckCallFilter {
	const IDENTIFIER: &'static str = "CheckCallFilter";
	type Implicit = ();
	type Val = ();
	type Pre = ();

	fn validate(
		&self,
		origin: RuntimeOrigin,
		call: &RuntimeCall,
		_info: &DispatchInfoOf<RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> ValidateResult<Self::Val, RuntimeCall> {
		// If allowed by the filter, accept
		if CallFilter::contains(call) {
			let validity = ValidTransaction::default();
			Ok((validity, (), origin))
		} else {
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
		}
	}

	impl_tx_ext_default!(RuntimeCall; weight prepare);
}
