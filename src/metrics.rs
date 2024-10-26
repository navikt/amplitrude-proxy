use once_cell::sync::Lazy;

use prometheus::{
	register_gauge, register_histogram, Gauge, Histogram, HistogramOpts, IntCounterVec,
};
use prometheus::{register_int_counter, register_int_counter_vec, IntCounter};

pub static INCOMING_REQUESTS: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("incoming_requests_total", "incoming requests").unwrap());

pub static HANDLED_REQUESTS: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("handled_requests_total", "handled requests").unwrap());

pub static PROXY_ERRORS: Lazy<IntCounterVec> = Lazy::new(|| {
	register_int_counter_vec!("proxy_errors_total", "proxys error", &["error"]).unwrap()
});

pub static INVALID_PEER: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("invalid_peer_total", "invalid peer").unwrap());

pub static INGRESS_COUNT: Lazy<Gauge> =
	Lazy::new(|| register_gauge!("ingress_count", "Number of ingresses in the cache").unwrap());

pub static UPSTREAM_500: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("upstream_500_total", "server errors").unwrap());

pub static UPSTREAM_400: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("upstream_400_total", "client errors").unwrap());

pub static UPSTREAM_200: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("upstream_200_total", "amplitude 200 ok").unwrap());

pub static COLLECT: Lazy<IntCounter> =
	Lazy::new(|| register_int_counter!("collect_endpoint_total", "collect endpoint").unwrap());

pub static COLLECT_AUTO: Lazy<IntCounter> = Lazy::new(|| {
	register_int_counter!("collect_auto_endpoint_total", "collect-auto endpoint").unwrap()
});

pub static REQUEST_DURATION: Lazy<Histogram> = Lazy::new(|| {
	let opts = HistogramOpts::new("request_duration_seconds", "Request duration in seconds")
		.buckets(vec![
			0.00025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
		]);
	register_histogram!(opts).unwrap()
});
