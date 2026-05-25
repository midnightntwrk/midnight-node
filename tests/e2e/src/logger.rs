//! Test logger setup.
//!
//! Each `#[tokio::test]` should start with `let _g = e2e_test!("test_name");`.
//! That installs a global `tracing-subscriber` (once) and enters a span tagged
//! with the test name so every line emitted by the test is prefixed with the
//! name. Without this, parallel test output interleaves and is very hard to
//! follow.
//!
//! Output format (compact, with uptime relative to process start):
//!
//! ```text
//!   12.345s  INFO test_name: message body here
//! ```
//!
//! Override the filter at the command line: `E2E_LOG=debug cargo test ...`.

use std::sync::Once;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::uptime;

static INIT: Once = Once::new();

/// Initialise the global tracing subscriber. Idempotent and cheap to call from
/// every test.
pub fn init() {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_env("E2E_LOG").unwrap_or_else(|_| {
            EnvFilter::new("info,subxt=warn,jsonrpsee=warn,hyper=warn,reqwest=warn")
        });
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_level(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_timer(uptime())
            .with_writer(std::io::stdout)
            .compact()
            .try_init();
    });
}

/// Initialise logging and enter a span tagged with the test name. Hold the
/// returned guard for the duration of the test:
///
/// ```ignore
/// #[tokio::test]
/// async fn my_test() {
///     let _g = e2e_test!("my_test");
///     tracing::info!("doing things");
/// }
/// ```
#[macro_export]
macro_rules! e2e_test {
    ($name:literal) => {{
        $crate::logger::init();
        ::tracing::info_span!($name).entered()
    }};
}
