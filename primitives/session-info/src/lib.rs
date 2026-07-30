//! Runtime API exposing the substrate session index.

#![cfg_attr(not(feature = "std"), no_std)]

sp_api::decl_runtime_apis! {
	pub trait SessionInfoApi {
		fn current_session_index() -> u32;
	}
}
