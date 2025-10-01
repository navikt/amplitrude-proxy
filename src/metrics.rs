use std::sync::LazyLock;

use prometheus::{
	Gauge, Histogram, HistogramOpts, IntCounterVec, register_gauge, register_histogram,
};
use prometheus::{IntCounter, register_int_counter, register_int_counter_vec};

pub static INCOMING_REQUESTS: LazyLock<IntCounter> = LazyLock::new(|| {
	register_int_counter!("incoming_requests_total", "incoming requests").unwrap()
});

pub static HANDLED_REQUESTS: LazyLock<IntCounter> =
	LazyLock::new(|| register_int_counter!("handled_requests_total", "handled requests").unwrap());

pub static PROXY_ERRORS: LazyLock<IntCounterVec> = LazyLock::new(|| {
	register_int_counter_vec!("proxy_errors_total", "proxys error", &["error"]).unwrap()
});

pub static DEFAULT_KEY: LazyLock<IntCounter> =
	LazyLock::new(|| register_int_counter!("default_key_total", "default key").unwrap());

pub static INVALID_PEER: LazyLock<IntCounter> =
	LazyLock::new(|| register_int_counter!("invalid_peer_total", "invalid peer").unwrap());

pub static INGRESS_COUNT: LazyLock<Gauge> =
	LazyLock::new(|| register_gauge!("ingress_count", "Number of ingresses in the cache").unwrap());

pub static UPSTREAM_500: LazyLock<IntCounter> =
	LazyLock::new(|| register_int_counter!("upstream_500_total", "server errors").unwrap());

pub static UPSTREAM_400: LazyLock<IntCounter> =
	LazyLock::new(|| register_int_counter!("upstream_400_total", "client errors").unwrap());

pub static UPSTREAM_200: LazyLock<IntCounter> =
	LazyLock::new(|| register_int_counter!("upstream_200_total", "amplitude 200 ok").unwrap());

pub static COLLECT: LazyLock<IntCounter> =
	LazyLock::new(|| register_int_counter!("collect_endpoint_total", "collect endpoint").unwrap());

pub static COLLECT_AUTO: LazyLock<IntCounter> = LazyLock::new(|| {
	register_int_counter!("collect_auto_endpoint_total", "collect-auto endpoint").unwrap()
});

pub static REQUEST_DURATION: LazyLock<Histogram> = LazyLock::new(|| {
	let opts = HistogramOpts::new("request_duration_seconds", "Request duration in seconds")
		.buckets(vec![
			0.00025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
		]);
	register_histogram!(opts).unwrap()
});
