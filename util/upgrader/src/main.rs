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

use actix_web::middleware::Logger;
use actix_web::{App, HttpResponse, HttpServer, Responder, get, web};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use subxt_signer::sr25519::Keypair;
use tokio::sync::Mutex;
use upgrader::error::UpgraderError;
use upgrader::{execute_upgrade, get_signer};

#[derive(Parser, Clone)]
#[command(version, about, long_about = None)]
struct Cli {
	/// The path to the new runtime WASM file
	#[arg(long, value_name = "FILE", env)]
	runtime_path: PathBuf,

	/// Seed for applying the authorized upgrade (can be any authority member)
	#[arg(short, long, env, default_value = "//Alice")]
	signer_key: String,

	/// Activate upgrade after a timeout (seconds)
	#[arg(short, long, env)]
	timeout: Option<u64>,

	/// RPC URL for sending the upgrade
	#[arg(short, long, default_value = "ws://localhost:9944", env)]
	rpc_url: String,

	/// Listen for HTTP requests on this port
	#[arg(short, long, default_value = "8080", env)]
	port: u16,
}

#[derive(Clone)]
struct AppData {
	pub rpc_url: String,
	pub signer: Keypair,
	pub code: Vec<u8>,
	pub already_executed: Arc<Mutex<bool>>,
	pub busy: Arc<Mutex<bool>>,
}

#[get("/execute")]
async fn execute(data: web::Data<AppData>) -> Result<impl Responder, UpgraderError> {
	if *data.already_executed.lock().await {
		Ok(HttpResponse::Conflict().body("upgrade has already been executed"))
	} else {
		*data.busy.lock().await = true;
		execute_upgrade(&data.rpc_url, &data.signer, &data.code).await?;
		*data.already_executed.lock().await = true;
		Ok(HttpResponse::Ok().body("upgrade executed"))
	}
}

#[get("/")]
async fn health() -> impl Responder {
	HttpResponse::Ok().body("ok")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let cli = Cli::parse();

	env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

	let signer = get_signer(&cli.signer_key).expect("failed to get signer");
	let code = std::fs::read(&cli.runtime_path)?;

	log::info!("Loaded new runtime code from path: {}", cli.runtime_path.display());
	if let Some(timeout) = cli.timeout {
		log::info!("Sleeping for {timeout} seconds...");
		std::thread::sleep(Duration::from_secs(timeout));
		execute_upgrade(&cli.rpc_url, &signer, &code)
			.await
			.expect("failed to execute upgrade");
		Ok(())
	} else {
		let port = cli.port;
		let app_data = AppData {
			rpc_url: cli.rpc_url,
			signer,
			code,
			already_executed: Arc::new(Mutex::new(false)),
			busy: Arc::new(Mutex::new(false)),
		};
		HttpServer::new(move || {
			App::new()
				.app_data(web::Data::new(app_data.clone()))
				.wrap(Logger::default())
				.service(execute)
				.service(health)
		})
		.bind(("0.0.0.0", port))?
		.run()
		.await
	}
}
