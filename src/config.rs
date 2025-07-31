use std::env;

#[derive(Clone, Debug)]
pub struct Upstream {
	pub host: String,
	pub sni: Option<String>,
	pub port: String,
}

#[derive(Clone, Debug)]
pub struct Config {
	pub upstream_amplitude: Upstream,
	pub amplitude_api_key_dev: String,
	pub amplitude_api_key_prod: String,
}

impl Config {
	pub fn new() -> Self {
		Self {
			upstream_amplitude: Upstream {
				host: env::var("AMPLITUDE_HOST").expect("Env var 'AMPLITUDE_HOST' needs to be set"),
				sni: env::var("AMPLITUDE_SNI").ok(),
				port: env::var("AMPLITUDE_PORT").expect("Env var 'AMPLITUDE_PORT' needs to be set"),
			},
			amplitude_api_key_dev: env::var("AMPLITUDE_API_KEY_DEV")
				.expect("Env var 'AMPLITUDE_API_KEY_DEV' needs to be set"),
			amplitude_api_key_prod: env::var("AMPLITUDE_API_KEY_PROD")
				.expect("Env var 'AMPLITUDE_API_KEY_PROD' needs to be set"),
		}
	}
}
