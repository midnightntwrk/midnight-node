//! Test logger setup.
//!
//! Tests annotated with `#[e2e_test]` (re-exported from this crate) get a
//! per-test tracing span automatically; this module just exposes the
//! subscriber initialiser the attribute expansion calls into.
//!
//! Output format (compact, UTC wall-clock + uptime relative to process start):
//!
//! ```text
//! 14:23:45.789   12.345s  INFO test_name: message body here
//! ```
//!
//! Override the filter at the command line: `E2E_LOG=debug cargo test ...`.

use std::sync::Once;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

static INIT: Once = Once::new();

/// Timer that renders `HH:MM:SS.mmm {secs}.{ms:03}s` — UTC wall-clock so log
/// lines can be cross-referenced against external systems, plus uptime since
/// the first test started so relative timing is obvious at a glance.
///
/// Both halves are millisecond precision. The built-in `Uptime` renders
/// nanoseconds (`{secs}.{:09}s`), which is noise for e2e timings.
struct E2eClock {
    start: Instant,
}

impl Default for E2eClock {
    fn default() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl FormatTime for E2eClock {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        let ms = now.subsec_millis();
        let up = self.start.elapsed();
        write!(
            w,
            "{:02}:{:02}:{:02}.{:03} {:>4}.{:03}s",
            h,
            m,
            s,
            ms,
            up.as_secs(),
            up.subsec_millis()
        )
    }
}

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
            .with_timer(E2eClock::default())
            .with_writer(std::io::stdout)
            .compact()
            .try_init();
    });
}
