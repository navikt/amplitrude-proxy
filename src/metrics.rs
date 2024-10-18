use once_cell::sync::Lazy;

use prometheus::{register_gauge, Gauge, IntCounterVec};
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
