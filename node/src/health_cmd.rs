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

#![allow(clippy::result_large_err)]

use clap::Parser;
use std::{
	io::{Read, Write},
	net::{Ipv4Addr, SocketAddr, TcpStream},
	time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(5);

/// Exit 0 if the node's RPC `/health` endpoint answers 200: the image's HEALTHCHECK,
/// which has no curl to call.
#[derive(Debug, Clone, Parser)]
pub struct HealthCmd {
	/// RPC port of the node to probe on 127.0.0.1.
	#[arg(long, env = "RPC_PORT", default_value_t = 9944)]
	pub port: u16,
}

impl HealthCmd {
	pub fn run(&self) -> sc_cli::Result<()> {
		let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
		let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)?;
		stream.set_read_timeout(Some(TIMEOUT))?;
		stream.set_write_timeout(Some(TIMEOUT))?;
		write!(
			stream,
			"GET /health HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
			self.port
		)?;

		let mut status_line = [0u8; 12];
		stream.read_exact(&mut status_line)?;
		// `HTTP/1.1 200`
		if status_line.starts_with(b"HTTP/") && status_line.ends_with(b" 200") {
			return Ok(());
		}
		Err(sc_cli::Error::Input(format!(
			"/health on {addr}: {}",
			String::from_utf8_lossy(&status_line)
		)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::net::TcpListener;

	fn probe(response: &'static [u8]) -> sc_cli::Result<()> {
		let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
		let port = listener.local_addr().unwrap().port();
		let server = std::thread::spawn(move || {
			let (mut conn, _) = listener.accept().unwrap();
			let mut request = [0u8; 256];
			let n = conn.read(&mut request).unwrap();
			assert!(request[..n].starts_with(b"GET /health HTTP/1.1\r\n"));
			conn.write_all(response).unwrap();
		});
		let result = HealthCmd { port }.run();
		server.join().unwrap();
		result
	}

	#[test]
	fn healthy_only_on_200() {
		assert!(probe(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n").is_ok());
		assert!(probe(b"HTTP/1.1 500 Internal Server Error\r\n\r\n").is_err());
		assert!(probe(b"nope").is_err());
	}

	#[test]
	fn unhealthy_when_nothing_listens() {
		let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
			.unwrap()
			.local_addr()
			.unwrap()
			.port();
		assert!(HealthCmd { port }.run().is_err());
	}
}
