// This file is part of midnight-node.
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

use console::style;
use log::kv::{Key, Value};
use std::{
	collections::BTreeMap,
	io::{self, Write},
	sync::Mutex,
};
use structured_logger::Writer;

/// A Writer implementation that outputs human-readable colored log lines.
///
/// Format: `LEVEL message key=value key=value ...`
pub struct PrettyWriter<W: Write + Sync + Send + 'static>(Mutex<Box<W>>);

impl<W: Write + Sync + Send + 'static> PrettyWriter<W> {
	pub fn new(w: W) -> Self {
		Self(Mutex::new(Box::new(w)))
	}
}

impl<W: Write + Sync + Send + 'static> Writer for PrettyWriter<W> {
	fn write_log(&self, value: &BTreeMap<Key, Value>) -> Result<(), io::Error> {
		let level = value.get(&Key::from_str("level")).map(|v| v.to_string());
		let message = value.get(&Key::from_str("message")).map(|v| v.to_string());
		let level_str = level.as_deref().unwrap_or("INFO");

		let styled_level = match level_str {
			"ERROR" => style(format!("{level_str:<5}")).red().bold(),
			"WARN" => style(format!("{level_str:<5}")).yellow().bold(),
			"INFO" => style(format!("{level_str:<5}")).green().bold(),
			"DEBUG" => style(format!("{level_str:<5}")).blue(),
			"TRACE" => style(format!("{level_str:<5}")).dim(),
			_ => style(format!("{level_str:<5}")).white(),
		};

		let mut buf = Vec::with_capacity(256);
		write!(buf, "{styled_level}")?;

		if let Some(msg) = &message {
			if !msg.is_empty() {
				write!(buf, " {msg}")?;
			}
		}

		// Append extra key=value pairs (skip standard fields)
		for (k, v) in value {
			let key = k.as_str();
			match key {
				"level" | "message" | "target" | "timestamp" | "module" | "file" | "line" => {},
				_ => write!(buf, " {key}{}", style(format!("={v}")).dim())?,
			}
		}

		buf.write_all(b"\n")?;

		if let Ok(mut w) = self.0.lock() {
			w.as_mut().write_all(&buf)?;
		}
		Ok(())
	}
}

/// Creates a new `Box<dyn Writer>` with the PrettyWriter for a given std::io::Write instance.
pub fn new_writer<W: Write + Sync + Send + 'static>(w: W) -> Box<dyn Writer> {
	Box::new(PrettyWriter::new(w))
}
