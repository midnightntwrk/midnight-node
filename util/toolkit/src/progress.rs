// This file is part of midnight-node.
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

use console::Term;
use indicatif::{MultiProgress, ProgressBar};
use std::{borrow::Cow, sync::atomic::AtomicU64, time::Duration};

pub struct Spin {
	pub(super) spin: ProgressBar,
}

impl Spin {
	pub fn new(message: impl Into<Cow<'static, str>>) -> Self {
		let message = message.into();
		if !Term::stdout().is_term() {
			println!("◴ {}", &message);
		}

		let spin = ProgressBar::new_spinner();
		spin.enable_steady_tick(Duration::from_millis(100));
		spin.set_message(message);
		Self { spin }
	}

	pub fn finish(self, message: impl Into<Cow<'static, str>>) {
		let message = message.into();
		self.spin.finish_with_message(message.clone());

		if !Term::stdout().is_term() {
			println!("✓ {message}");
		}
	}
}

pub struct Progress {
	spin: ProgressBar,
	bar: ProgressBar,
	count: AtomicU64,
	#[allow(dead_code)]
	multi: MultiProgress,
}

impl Progress {
	pub fn new(count: usize, message: impl Into<Cow<'static, str>>) -> Self {
		let message = message.into();
		if !Term::stdout().is_term() {
			println!("◴ {}", &message);
		}

		let multi = MultiProgress::new();
		let spin = Spin::new(message);
		let spin = multi.add(spin.spin);
		let bar = multi.add(ProgressBar::new(count as u64));

		Self { spin, bar, count: AtomicU64::new(0), multi }
	}

	pub fn inc(&self, amount: usize) {
		self.bar.inc(amount as u64);
		let prev = self.count.fetch_add(amount as u64, std::sync::atomic::Ordering::Relaxed);

		if !Term::stdout().is_term() {
			let prev_tenth = ((prev as f32 / self.bar.length().unwrap() as f32) / 0.1).floor();
			let cur_tenth =
				(((prev as f32 + amount as f32) / self.bar.length().unwrap() as f32) / 0.1).floor();
			let message = self.spin.message();
			if cur_tenth > prev_tenth {
				println!("◴ {message} {:.0}%", cur_tenth * 10f32);
			}
		}
	}

	pub fn finish(self, message: impl Into<Cow<'static, str>>) {
		let message = message.into();
		self.bar.finish();
		self.spin.finish_with_message(message.clone());

		if !Term::stdout().is_term() {
			println!("✓ {}", &message);
		}
	}
}
