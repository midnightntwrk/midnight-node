// This file is part of midnight-indexer.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Telemetry: logging, tracing and metrics.
//!
//! ## Logging
//!
//! - Use the `log` crate with the `kv` feature for structured logging enabled.
//! - Fields are separated by comma and separated from the message by a single trailing semicolon:
//!   `info!(foo:?, bar:%, baz, qux:? = qoox; "message")`
//! - Use `:?` to use `Debug` and `:%` to use `Display` to "render" field values.
//! - For errors, include the error chain: `error!(error:% = error.as_chain(); "message")`.
//!
//! ## Tracing
//! - Use the `#[trace]` attribute to instrument functions/methods.
//! - To create a root span, use `Span::root("name", SpanContext::random())`.
//!
//! ## Metrics
//! - Metrics are exposed via Prometheus at the configured endpoint.

use fastrace::{
    collector::{ConsoleReporter, Reporter},
    prelude::SpanRecord,
};
use fastrace_opentelemetry::OpenTelemetryReporter;
use logforth::{
    append::{FastraceEvent, Stdout},
    diagnostic::FastraceDiagnostic,
    filter::EnvFilter,
    layout::JsonLayout,
};
use metrics_exporter_prometheus::PrometheusBuilder;
use opentelemetry::InstrumentationScope;
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use serde::Deserialize;
use std::{borrow::Cow, net::IpAddr};

/// Telemetry (tracing, metrics) configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(rename = "tracing")]
    pub tracing_config: TracingConfig,

    #[serde(rename = "metrics")]
    pub metrics_config: MetricsConfig,
}

/// Tracing configuration.
///
/// All fields have sensible deserialization defaults.
#[derive(Debug, Clone, Deserialize)]
pub struct TracingConfig {
    pub enabled: bool,

    pub service_name: String,

    pub otlp_exporter_endpoint: String,

    /// Should only be used for testing. Defaults to false.
    #[serde(default)]
    pub console_reporter_enabled: bool,

    /// Defaults to the package name.
    #[serde(default = "package_name")]
    pub instrumentation_scope_name: String,

    /// Defaults to the package version.
    #[serde(default = "package_version")]
    pub instrumentation_scope_version: String,
}

/// Metrics configuration.
///
/// All fields have sensible deserialization defaults.
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub address: IpAddr,
    pub port: u16,
}

/// Initialize logging with [Logforth](https://github.com/fast/logforth).
///
/// Log levels are filterd based on the `RUST_LOG` environment variable and log records are
/// formatted as JSON.
///
/// If logging happens in the context of a span, log records are added to the current span as events
/// and the trace ID of the current span is added to the log records, thus correlating logs and
/// traces.
///
/// # Panics
///
/// If logging has already been initialized.
pub fn init_logging() {
    logforth::builder()
        .dispatch(|dispatch| {
            dispatch
                .filter(EnvFilter::from_default_env())
                .diagnostic(FastraceDiagnostic::default())
                .append(Stdout::default().with_layout(JsonLayout::default()))
                .append(FastraceEvent::default())
        })
        .apply();
}

/// Initialize tracing with [fastrace](https://github.com/fast/fastrace).
///
/// Builds an OTLP exporter using gRPC from the given configuration.
///
/// # Panics
///
/// Panics if the OTLP exporter cannot be built.
pub fn init_tracing(config: TracingConfig) {
    if config.enabled {
        let TracingConfig {
            console_reporter_enabled,
            otlp_exporter_endpoint,
            service_name,
            instrumentation_scope_name,
            instrumentation_scope_version,
            ..
        } = config;

        let exporter = SpanExporter::builder()
            .with_tonic()
            .with_endpoint(otlp_exporter_endpoint)
            .build()
            .expect("OTLP exporter can be built");

        let resource = Resource::builder().with_service_name(service_name).build();

        let instrumentation_scope = InstrumentationScope::builder(instrumentation_scope_name)
            .with_version(instrumentation_scope_version)
            .build();

        let open_telemetry =
            OpenTelemetryReporter::new(exporter, Cow::Owned(resource), instrumentation_scope);
        let console_reporter = console_reporter_enabled.then_some(ConsoleReporter);
        let reporter = TracingReporter {
            open_telemetry,
            console_reporter,
        };

        fastrace::set_reporter(reporter, fastrace::collector::Config::default());
    }
}

/// Initialize metrics.
///
/// Installs a Prometheus exporter.
///
/// # Panics
///
/// Panics if the Prometheus exporter can be installed.
pub fn init_metrics(config: MetricsConfig) {
    let MetricsConfig {
        enabled,
        address,
        port,
    } = config;

    if enabled {
        PrometheusBuilder::new()
            .with_http_listener((address, port))
            .install()
            .expect("Prometheus exporter can be installed");
    }
}

struct TracingReporter {
    open_telemetry: OpenTelemetryReporter,
    console_reporter: Option<ConsoleReporter>,
}

impl Reporter for TracingReporter {
    fn report(&mut self, spans: Vec<SpanRecord>) {
        if let Some(ref mut console_reporter) = self.console_reporter {
            console_reporter.report(spans.clone());
        }

        self.open_telemetry.report(spans);
    }
}

fn package_name() -> String {
    env!("CARGO_PKG_NAME").to_owned()
}

fn package_version() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}
