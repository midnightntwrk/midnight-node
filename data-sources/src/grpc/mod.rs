pub mod client;
mod conversions;
pub mod handle;
pub mod requests;

// Without `embedded`, generate the proto types locally; with it, reuse the
// indexer crate's generated module so the in-process service (which speaks
// those exact types) can be called with no conversion layer. The two are
// built from the same proto definition.
#[cfg(not(feature = "embedded"))]
pub mod midnight_state {
	tonic::include_proto!("midnight_state");
}
#[cfg(feature = "embedded")]
pub use acropolis_module_midnight_state::grpc::midnight_state_proto as midnight_state;
