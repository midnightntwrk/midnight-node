use super::RuntimeCall;
use frame_support::traits::Contains;

/// Filter that whitelists Governance calls.
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
