use std::fs;
use std::net::ToSocketAddrs;

use pingora::services::listening::Service;
use pingora::{prelude::Opt, proxy as pingora_proxy, server::Server};
use tracing::info;
mod annotate;
mod config;
mod health;
mod proxy;
mod trace;

use once_cell::sync::Lazy;
use prometheus::{register_int_counter, IntCounter};

static INCOMING_REQUESTS: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("incoming_requests_total", "incoming requests").unwrap());

static HANDLED_REQUESTS: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("handled_requests_total", "handled requests").unwrap());

static ERRORS_WHILE_PROXY: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("error_while_proxy_total", "error while proxy").unwrap());

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

	trace::init();
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


	let mut probe_instance =
		pingora_proxy::http_proxy_service(&amplitrude_proxy.configuration, health::Probes {});
	let mut proxy_instance = pingora_proxy::http_proxy_service(
		&amplitrude_proxy.configuration,
		/* We test against this server
		socat \
			-v -d -d \
			TCP-LISTEN:1234,crlf,reuseaddr,fork \
			SYSTEM:"
				echo HTTP/1.1 200 OK;
				echo Content-Type\: text/plain;
				echo;
			"

				 */
		proxy::AmplitudeProxy {
			conf: conf.clone(),
			addr: format!(
				"{}:{}",
				conf.upstream_amplitude.host.to_owned(),
				conf.upstream_amplitude.port.to_owned()
			)
			.to_socket_addrs()
			.unwrap()
			.next()
			.unwrap(),
			sni: conf.upstream_amplitude.sni.to_owned(),
		},
	);

	let mut prome_service_http = Service::prometheus_http_service();
	prome_service_http.add_tcp("0.0.0.0:9090");
	probe_instance.add_tcp("0.0.0.0:6969");
	proxy_instance.add_tcp("0.0.0.0:6191");
	amplitrude_proxy.add_service(probe_instance);
	amplitrude_proxy.add_service(proxy_instance);
	amplitrude_proxy.add_service(prome_service_http);
	amplitrude_proxy.run_forever();
}
