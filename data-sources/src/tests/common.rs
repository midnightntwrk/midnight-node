use std::{error::Error, fmt, str::FromStr, time::Duration};

pub const CNIGHT_OBSERVATION_POOL_CFG: DbPoolCfg =
	DbPoolCfg { acquire_timeout: Duration::from_secs(30), max_connections: 10 };

pub const STANDARD_POOL_CFG: DbPoolCfg =
	DbPoolCfg { acquire_timeout: Duration::from_secs(30), max_connections: 5 };

pub async fn get_connection(
	connection_string: &str,
	pool_cfg: DbPoolCfg,
	allow_non_ssl: bool,
) -> Result<sqlx::PgPool, Box<dyn Error + Send + Sync + 'static>> {
	let connect_options =
		sqlx::postgres::PgConnectOptions::from_str(connection_string)?.ssl_mode(if allow_non_ssl {
			sqlx::postgres::PgSslMode::Disable
		} else {
			sqlx::postgres::PgSslMode::Require
		});

	let pool = sqlx::postgres::PgPoolOptions::new()
		.max_connections(pool_cfg.max_connections)
		.acquire_timeout(pool_cfg.acquire_timeout)
		.connect_with(connect_options.clone())
		.await
		.map_err(|e| {
			PostgresConnectionError(
				connect_options.get_host().to_string(),
				connect_options.get_port(),
				connect_options.get_database().unwrap_or("cexplorer").to_string(),
				e.to_string(),
			)
			.to_string()
		})?;
	Ok(pool)
}

#[derive(Clone, Copy)]
pub struct DbPoolCfg {
	acquire_timeout: Duration,
	max_connections: u32,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Could not connect to database: postgres://***:***@{0}:{1}/{2}; error: {3}")]
pub struct PostgresConnectionError(String, u16, String, String);

impl fmt::Debug for DbPoolCfg {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("DbPoolCfg")
			.field("acquire_timeout", &self.acquire_timeout)
			.field("max_connections", &self.max_connections)
			.finish()
	}
}
