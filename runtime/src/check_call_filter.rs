use super::{RuntimeCall, RuntimeOrigin};
use frame_support::{pallet_prelude::TransactionSource, traits::Contains};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{DispatchInfoOf, TransactionExtension, ValidateResult},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
};

/// Filter that whitelists Governance calls
pub struct GovernanceAuthorityCallFilter;
impl Contains<RuntimeCall> for GovernanceAuthorityCallFilter {
	fn contains(call: &RuntimeCall) -> bool {
		matches!(
			call,
			RuntimeCall::Council(_)
				| RuntimeCall::TechnicalCommittee(_)
				| RuntimeCall::FederatedAuthority(
					pallet_federated_authority::Call::motion_close { .. }
				) | RuntimeCall::System(frame_system::Call::apply_authorized_upgrade { .. })
		)
	}
}

/// The runtime's inherent calls, whitelisted in safe mode.
///
/// `BaseCallFilter` filters every non-Root origin, including the None origin inherents
/// dispatch with. A filtered Mandatory inherent means `BadMandatory`, i.e. no valid blocks
/// can be built, so every inherent must be whitelisted for the chain to stay live while
/// safe mode is entered. `Midnight::send_mn_transaction` (the unsigned user-transaction
/// entry point) is deliberately NOT listed: filtering user traffic is the point of safe mode.
pub struct InherentCalls;
impl Contains<RuntimeCall> for InherentCalls {
	fn contains(call: &RuntimeCall) -> bool {
		matches!(
			call,
			RuntimeCall::Timestamp(pallet_timestamp::Call::set { .. })
				| RuntimeCall::SessionCommitteeManagement(
					pallet_session_validator_management::Call::set { .. }
				) | RuntimeCall::CNightObservation(
				pallet_cnight_observation::Call::process_tokens { .. }
			) | RuntimeCall::Bridge(pallet_partner_chains_bridge::Call::handle_transfers { .. })
				| RuntimeCall::FederatedAuthorityObservation(
					pallet_federated_authority_observation::Call::reset_members { .. }
				)
		)
	}
}

/// Calls the collectives dispatch internally (with their `Members` origin) when one of
/// their proposals passes, whitelisted in safe mode.
///
/// `pallet_collective` dispatches an approved proposal through the origin's call filter,
/// and its `Members` origin is not Root, so `BaseCallFilter` applies. Without these the
/// safe-mode recovery flow dead-ends: neither body could record a
/// `FederatedAuthority::motion_approve`, so no motion could ever be `motion_close`d to
/// dispatch a fix as Root. Both calls are origin-gated to the collective proportion
/// origins, so whitelisting them adds no exposure for signed or unsigned traffic.
pub struct FederatedMotionCalls;
impl Contains<RuntimeCall> for FederatedMotionCalls {
	fn contains(call: &RuntimeCall) -> bool {
		matches!(
			call,
			RuntimeCall::FederatedAuthority(
				pallet_federated_authority::Call::motion_approve { .. }
					| pallet_federated_authority::Call::motion_revoke { .. }
			)
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
