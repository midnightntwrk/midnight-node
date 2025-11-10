use core::ops::Div;
use frame_support::{
	dispatch::DispatchResult,
	traits::{
		fungible::{
			Inspect as FungibleInspect, Mutate as FungibleMutate,
			InspectHold as FungibleInspectHold, MutateHold as FungibleMutateHold,
			Unbalanced as FungibleUnbalanced, UnbalancedHold as FungibleUnbalancedHold,
		},
		BalanceStatus, Currency, ExistenceRequirement, Imbalance, ReservableCurrency, SameOrOther,
		SignedImbalance, 
		tokens::{DepositConsequence, Fortitude, Precision, Preservation, Provenance, Restriction, WithdrawConsequence},
		TryDrop, WithdrawReasons, tokens::imbalance::TryMerge,
	},
};
use pallet_session::HoldReason;
use sp_runtime::{DispatchError, traits::Saturating};

pub struct CurrencyWaiver;

#[derive(Default)]
pub struct ImbalanceWaiver;

impl TryDrop for ImbalanceWaiver {
	fn try_drop(self) -> Result<(), Self> {
		Ok(())
	}
}

impl TryMerge for ImbalanceWaiver {
	fn try_merge(self, _: Self) -> Result<Self, (Self, Self)> {
		Ok(ImbalanceWaiver)
	}
}

impl<Balance: Default> Imbalance<Balance> for ImbalanceWaiver {
	type Opposite = ImbalanceWaiver;
	fn zero() -> Self {
		ImbalanceWaiver
	}
	fn drop_zero(self) -> Result<(), Self> {
		Ok(())
	}
	fn split(self, _: Balance) -> (Self, Self) {
		(ImbalanceWaiver, ImbalanceWaiver)
	}
	fn extract(&mut self, _: Balance) -> Self {
		ImbalanceWaiver
	}
	fn ration(self, _: u32, _: u32) -> (Self, Self)
	where
		Balance: From<u32> + Saturating + Div<Output = Balance>,
	{
		(ImbalanceWaiver, ImbalanceWaiver)
	}
	fn split_merge(self, _: Balance, _: (Self, Self)) -> (Self, Self) {
		(ImbalanceWaiver, ImbalanceWaiver)
	}
	fn ration_merge(self, _: u32, _: u32, _: (Self, Self)) -> (Self, Self)
	where
		Balance: From<u32> + Saturating + Div<Output = Balance>,
	{
		(ImbalanceWaiver, ImbalanceWaiver)
	}
	fn split_merge_into(self, _: Balance, _: &mut (Self, Self)) {}
	fn ration_merge_into(self, _: u32, _: u32, _: &mut (Self, Self))
	where
		Balance: From<u32> + Saturating + Div<Output = Balance>,
	{
	}
	fn merge(self, _: Self) -> Self {
		ImbalanceWaiver
	}
	fn merge_into(self, _: &mut Self) {}
	fn maybe_merge(self, _: Option<Self>) -> Self {
		ImbalanceWaiver
	}
	fn subsume(&mut self, _: Self) {}
	fn maybe_subsume(&mut self, _: Option<Self>) {}
	fn offset(self, _: Self::Opposite) -> SameOrOther<Self, Self::Opposite> {
		SameOrOther::None
	}
	fn peek(&self) -> Balance {
		Default::default()
	}
}

impl<AccountId> Currency<AccountId> for CurrencyWaiver {
	type Balance = u32;
	type PositiveImbalance = ImbalanceWaiver;
	type NegativeImbalance = ImbalanceWaiver;
	fn total_balance(_: &AccountId) -> Self::Balance {
		0
	}
	fn can_slash(_: &AccountId, _: Self::Balance) -> bool {
		true
	}
	fn total_issuance() -> Self::Balance {
		0
	}
	fn minimum_balance() -> Self::Balance {
		0
	}
	fn burn(_: Self::Balance) -> Self::PositiveImbalance {
		ImbalanceWaiver
	}
	fn issue(_: Self::Balance) -> Self::NegativeImbalance {
		ImbalanceWaiver
	}
	fn pair(_: Self::Balance) -> (Self::PositiveImbalance, Self::NegativeImbalance) {
		(ImbalanceWaiver, ImbalanceWaiver)
	}
	fn free_balance(_: &AccountId) -> Self::Balance {
		0
	}
	fn ensure_can_withdraw(
		_: &AccountId,
		_: Self::Balance,
		_: WithdrawReasons,
		_: Self::Balance,
	) -> DispatchResult {
		Ok(())
	}
	fn transfer(
		_: &AccountId,
		_: &AccountId,
		_: Self::Balance,
		_: ExistenceRequirement,
	) -> DispatchResult {
		Ok(())
	}
	fn slash(_: &AccountId, _: Self::Balance) -> (Self::NegativeImbalance, Self::Balance) {
		(ImbalanceWaiver, 0)
	}
	fn deposit_into_existing(
		_: &AccountId,
		_: Self::Balance,
	) -> Result<Self::PositiveImbalance, DispatchError> {
		Ok(ImbalanceWaiver)
	}
	fn resolve_into_existing(
		_: &AccountId,
		_: Self::NegativeImbalance,
	) -> Result<(), Self::NegativeImbalance> {
		Ok(())
	}
	fn deposit_creating(_: &AccountId, _: Self::Balance) -> Self::PositiveImbalance {
		ImbalanceWaiver
	}
	fn resolve_creating(_: &AccountId, _: Self::NegativeImbalance) {}
	fn withdraw(
		_: &AccountId,
		_: Self::Balance,
		_: WithdrawReasons,
		_: ExistenceRequirement,
	) -> Result<Self::NegativeImbalance, DispatchError> {
		Ok(ImbalanceWaiver)
	}
	fn settle(
		_: &AccountId,
		_: Self::PositiveImbalance,
		_: WithdrawReasons,
		_: ExistenceRequirement,
	) -> Result<(), Self::PositiveImbalance> {
		Ok(())
	}
	fn make_free_balance_be(
		_: &AccountId,
		_: Self::Balance,
	) -> SignedImbalance<Self::Balance, Self::PositiveImbalance> {
		SignedImbalance::Positive(ImbalanceWaiver)
	}
}

impl<AccountId> ReservableCurrency<AccountId> for CurrencyWaiver {
	fn can_reserve(_: &AccountId, _: Self::Balance) -> bool {
		true
	}
	fn slash_reserved(_: &AccountId, _: Self::Balance) -> (Self::NegativeImbalance, Self::Balance) {
		(ImbalanceWaiver, 0)
	}
	fn reserved_balance(_: &AccountId) -> Self::Balance {
		0
	}
	fn reserve(_: &AccountId, _: Self::Balance) -> DispatchResult {
		Ok(())
	}
	fn unreserve(_: &AccountId, _: Self::Balance) -> Self::Balance {
		0
	}
	fn repatriate_reserved(
		_: &AccountId,
		_: &AccountId,
		_: Self::Balance,
		_: BalanceStatus,
	) -> Result<Self::Balance, DispatchError> {
		Ok(0)
	}
}


impl<AccountId> FungibleInspect<AccountId> for CurrencyWaiver {
    type Balance = u128;

	fn total_balance(_: &AccountId) -> <Self as frame_support::traits::fungible::Inspect<AccountId>>::Balance { 0 }
    fn total_issuance() -> Self::Balance { 0 }
    fn minimum_balance() -> Self::Balance { 0 }
    fn balance(_: &AccountId) -> Self::Balance { 0 }
    fn reducible_balance(_: &AccountId, _: Preservation, _: Fortitude)
        -> Self::Balance
    { 0 }
    fn can_deposit(_: &AccountId, _: Self::Balance, _: Provenance) -> DepositConsequence {
        DepositConsequence::Success
    }
    fn can_withdraw(_: &AccountId, _: Self::Balance) -> WithdrawConsequence<u128> {
        WithdrawConsequence::Success
    }
}

impl<AccountId: Eq> FungibleMutate<AccountId> for CurrencyWaiver {
    fn mint_into(_: &AccountId, _: u128) -> Result<u128, DispatchError> { Ok(0) }
    fn burn_from(_: &AccountId, _: u128, _: Preservation, _: Precision, _: Fortitude) -> Result<u128, DispatchError> { Ok(0) }
    fn transfer(_: &AccountId, _: &AccountId, _: u128, _: Preservation) -> Result<u128, DispatchError> { Ok(0) }
    fn set_balance(_: &AccountId, _: u128) -> u128 { 0 }
}

impl<AccountId> FungibleInspectHold<AccountId> for CurrencyWaiver {
    type Reason = HoldReason;
    fn balance_on_hold(_: &Self::Reason, _: &AccountId) -> u128 { 0 }
    fn reducible_total_balance_on_hold(
        _: &AccountId,
        _: Fortitude,
    ) -> u128 { 0 }
    fn can_hold(_: &Self::Reason, _: &AccountId, _: u128) -> bool { true }
    fn hold_available(_: &Self::Reason, _: &AccountId) -> bool { true }
	fn total_balance_on_hold(_: &AccountId) -> Self::Balance { 0 }
}

impl<AccountId> FungibleMutateHold<AccountId> for CurrencyWaiver {
    fn hold(_: &HoldReason, _: &AccountId, _: u128) -> Result<(), DispatchError> { Ok(()) }
    fn release(_: &HoldReason, _: &AccountId, _: u128, _: Precision) -> Result<u128, DispatchError> { Ok(0) }
}

impl<AccountId> FungibleUnbalanced<AccountId> for CurrencyWaiver {
	fn handle_dust(dust: frame_support::traits::fungible::Dust<AccountId, Self>) {
		// Do nothing
	}
    fn write_balance(_: &AccountId, _: <Self as frame_support::traits::fungible::Inspect<AccountId>>::Balance) -> Result<core::option::Option<<Self as frame_support::traits::fungible::Inspect<AccountId>>::Balance>, sp_runtime::DispatchError> { todo!() }
    fn set_total_issuance(_: <Self as frame_support::traits::fungible::Inspect<AccountId>>::Balance) { 
		// Do nothing
	 }

}

impl<AccountId> FungibleUnbalancedHold<AccountId> for CurrencyWaiver {
	fn set_balance_on_hold(
			reason: &Self::Reason,
			who: &AccountId,
			amount: Self::Balance,
		) -> sp_runtime::DispatchResult {
		Ok(())
	}
}
