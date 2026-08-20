// This file is part of midnight-ledger.
// Copyright (C) Midnight Foundation
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

use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::filter::targets::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry};

pub enum LogLevel {
    Off,
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        use LogLevel::*;
        match level {
            Off => LevelFilter::OFF,
            Trace => LevelFilter::TRACE,
            Debug => LevelFilter::DEBUG,
            Info => LevelFilter::INFO,
            Warn => LevelFilter::WARN,
            Error => LevelFilter::ERROR,
        }
    }
}

pub fn init_logger(level: LogLevel) {
    Registry::default()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(Targets::new().with_default(level)),
        )
        .try_init()
        .ok();
    info!("Welcome to ledger v{}!", env!("CARGO_PKG_VERSION"));
}
