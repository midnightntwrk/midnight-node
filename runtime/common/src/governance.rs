use frame_support::traits::{ChangeMembers, InitializeMembers};
use pallet_collective::{DefaultVote, MemberCount};
use sp_std::marker::PhantomData;

/// Wrapper struct to handle frame_system sufficients and delegate
/// `InitializeMembers` and `ChangeMembers` calls to `P`.
pub struct MembershipHandler<T, P>(PhantomData<(T, P)>)
where
	T: frame_system::Config,
	P: InitializeMembers<T::AccountId> + ChangeMembers<T::AccountId>;

impl<T, P> InitializeMembers<T::AccountId> for MembershipHandler<T, P>
where
	T: frame_system::Config,
	P: InitializeMembers<T::AccountId> + ChangeMembers<T::AccountId>,
{
	fn initialize_members(members: &[T::AccountId]) {
		// First, delegate to P's implementation
		<P as InitializeMembers<T::AccountId>>::initialize_members(members);

		// Then, increase sufficients for all members
		for who in members {
			frame_system::Pallet::<T>::inc_sufficients(who);
		}
	}
}

impl<T, P> ChangeMembers<T::AccountId> for MembershipHandler<T, P>
where
	T: frame_system::Config,
	P: ChangeMembers<T::AccountId> + InitializeMembers<T::AccountId>,
{
	fn change_members_sorted(
		incoming: &[T::AccountId],
		outgoing: &[T::AccountId],
		new: &[T::AccountId],
	) {
		// First, delegate to P's implementation
		<P as ChangeMembers<T::AccountId>>::change_members_sorted(incoming, outgoing, new);

		// Then, handle sufficients
		for who in incoming {
			frame_system::Pallet::<T>::inc_sufficients(who);
		}
		for who in outgoing {
			frame_system::Pallet::<T>::dec_sufficients(who);
		}
	}
}

/// Default votes will be always NO for abstentions
pub struct AlwaysNo;
impl DefaultVote for AlwaysNo {
	fn default_vote(
		_prime_vote: Option<bool>,
		_yes_votes: MemberCount,
		_no_votes: MemberCount,
		_len: MemberCount,
	) -> bool {
		false
	}
}
