use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::{Arc, Mutex};

use pingora::services::listening::Service;
use pingora::{prelude::Opt, proxy as pingora_proxy, server::Server};
use tracing::info;
mod annotate;
mod cache;
mod config;
mod health;
mod k8s;
mod proxy;
mod trace;
use once_cell::sync::Lazy;
use prometheus::{register_int_counter, IntCounter};
use tracing_subscriber::util::SubscriberInitExt;

static INCOMING_REQUESTS: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("incoming_requests_total", "incoming requests").unwrap());

static HANDLED_REQUESTS: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("handled_requests_total", "handled requests").unwrap());

static ERRORS_WHILE_PROXY: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("error_while_proxy_total", "error while proxy").unwrap());

static CONNECTION_ERRORS: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("connection_errors_total", "connection errors").unwrap());

static SSL_ERROR: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("ssl_errors_total", "ssl errors").unwrap());

static UPSTREAM_CONNECTION_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
	register_int_counter!(
		"upstream_connection_failures_total",
		"upstream connection failure"
	)
	.unwrap()
});

fn main() {
	let conf = config::Config::new();

	let logging = match trace::configure_logging() {
		Ok(l) => l,
		Err(e) => {
			eprintln!("Error: {e}");
			e.chain()
				.skip(1)
				.for_each(|cause| eprintln!("because: {}", cause));
			std::process::exit(1);
		},
	};
	logging.init();
	info!("started proxy{:#?}", &conf);
	let mut amplitrude_proxy = Server::new(Some(Opt {
		upgrade: false,
		daemon: false,
		nocapture: false,
		test: false,
		conf: None,
	}))
	.unwrap();
	amplitrude_proxy.bootstrap();

	let proxy = proxy::AmplitudeProxy::new(
		conf.clone(),
		format!(
			"{}:{}",
			conf.upstream_amplitude.host.to_owned(),
			conf.upstream_amplitude.port.to_owned()
		)
		.to_socket_addrs()
		.unwrap()
		.next()
		.unwrap(),
		conf.upstream_amplitude.sni.to_owned(),
		2000, // There's about ~1000 ingresses as of Wed Oct  2 16:23:24 CEST 2024
	);

	let mut probe_instance =
		pingora_proxy::http_proxy_service(&amplitrude_proxy.configuration, health::Probes {});
	let mut proxy_instance =
		pingora_proxy::http_proxy_service(&amplitrude_proxy.configuration, proxy);

	let mut prome_service_http = Service::prometheus_http_service();
	prome_service_http.add_tcp("0.0.0.0:9090");
	probe_instance.add_tcp("0.0.0.0:6969");
	proxy_instance.add_tcp("0.0.0.0:6191");
	amplitrude_proxy.add_service(probe_instance);
	amplitrude_proxy.add_service(proxy_instance);
	amplitrude_proxy.add_service(prome_service_http);
	amplitrude_proxy.run_forever();
}
