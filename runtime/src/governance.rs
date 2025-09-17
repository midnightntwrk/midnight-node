use frame_support::traits::{ChangeMembers, EnsureOrigin, InitializeMembers, PalletInfoAccess};
use pallet_federated_authority::AuthorityOriginInfo;
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
		// First, delegate to T's implementation
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
		// First, delegate to T's implementation
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

/// A type-level struct to hold the specification for a single federated authority.
/// - `I`: The pallet's Instance marker type.
/// - `P`: The pallet type itself (from `construct_runtime!`).
/// - `N`, `D`: The numerator and denominator for the proportion.
pub struct AuthorityBody<I, P, const N: u32, const D: u32> {
	_phantom: PhantomData<(I, P)>,
}

/// Helper trait to check an origin against an `AuthorityBodySpec`.
trait EnsureFromIdentity<AccountId, O> {
	/// On success, returns the pallet index of the authority that matched.
	fn ensure_from_body(o: O) -> Result<AuthorityOriginInfo, O>;
}

use pallet_collective::EnsureProportionAtLeast;

impl<AccountId, O, I, P, const N: u32, const D: u32> EnsureFromIdentity<AccountId, O>
	for AuthorityBody<I, P, N, D>
where
	O: Clone,
	I: 'static,
	P: PalletInfoAccess,
	EnsureProportionAtLeast<AccountId, I, N, D>: EnsureOrigin<O>,
{
	fn ensure_from_body(o: O) -> Result<AuthorityOriginInfo, O> {
		EnsureProportionAtLeast::<AccountId, I, N, D>::try_origin(o).map(|_| AuthorityOriginInfo {
			id: P::index() as u8,
			n: N,
			d: D,
		})
	}
}

#[impl_trait_for_tuples::impl_for_tuples(5)]
impl<AccountId, O: Clone> EnsureFromIdentity<AccountId, O> for Tuple {
	fn ensure_from_body(o: O) -> Result<AuthorityOriginInfo, O> {
		for_tuples!( #(
            match Tuple::ensure_from_body(o.clone()) {
                Ok(auth_origin_info) => return Ok(auth_origin_info),
                Err(_) => {}
            }
        )* );
		Err(o)
	}
}

/// A generic `EnsureOrigin` implementation that checks an origin against a list
/// of authority specifications provided in a tuple.
pub struct FederatedAuthorityOriginManager<AccountId, Authorities, const N: u32, const D: u32>(
	PhantomData<(AccountId, Authorities)>,
);

impl<O, AccountId, Authorities, const N: u32, const D: u32> EnsureOrigin<O>
	for FederatedAuthorityOriginManager<AccountId, Authorities, N, D>
where
	O: Clone,
	Authorities: EnsureFromIdentity<AccountId, O>,
{
	type Success = AuthorityOriginInfo;

	fn try_origin(o: O) -> Result<Self::Success, O> {
		Authorities::ensure_from_body(o)
	}
}
