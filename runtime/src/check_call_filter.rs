use super::RuntimeCall;
use frame_support::traits::Contains;

/// Filter that whitelists governance calls.
pub struct GovernanceAuthorityCallFilter;

impl Contains<RuntimeCall> for GovernanceAuthorityCallFilter {
	fn contains(call: &RuntimeCall) -> bool {
		matches!(
			call,
			RuntimeCall::Council(_)
				| RuntimeCall::TechnicalCommittee(_)
				| RuntimeCall::FederatedAuthority(
					pallet_federated_authority::Call::motion_close { .. }
				)
				| RuntimeCall::System(frame_system::Call::apply_authorized_upgrade { .. })
		)
	}
}

/// Filter that whitelists active unsigned calls.
pub struct ActiveUnsignedCallFilter;

impl Contains<RuntimeCall> for ActiveUnsignedCallFilter {
	fn contains(call: &RuntimeCall) -> bool {
		matches!(
			call,
			RuntimeCall::Timestamp(pallet_timestamp::Call::set { .. })
				| RuntimeCall::Midnight(pallet_midnight::Call::send_mn_transaction { .. })
				| RuntimeCall::SessionCommitteeManagement(
					pallet_session_validator_management::Call::set { .. }
				)
				| RuntimeCall::Bridge(pallet_partner_chains_bridge::Call::handle_transfers {
					..
				})
				| RuntimeCall::CNightObservation(
					pallet_cnight_observation::Call::process_tokens { .. }
				)
				| RuntimeCall::FederatedAuthorityObservation(
					pallet_federated_authority_observation::Call::reset_members { .. }
				)
		)
	}
}

pub type BaseRuntimeCallFilter = (GovernanceAuthorityCallFilter, ActiveUnsignedCallFilter);
