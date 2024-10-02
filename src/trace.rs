use std::{env::var, io::IsTerminal};

use opentelemetry::{trace::TracerProvider, KeyValue};
use opentelemetry_sdk::{
	runtime,
	trace::{self as sdktrace, Config, Tracer},
	Resource,
};
use opentelemetry_semantic_conventions::resource;
use tracing::{level_filters::LevelFilter, Level, Subscriber};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{filter, fmt, layer::SubscriberExt, Layer, Registry};

// https://github.com/tokio-rs/tracing-opentelemetry/blob/v0.1.x/examples/opentelemetry-otlp.rs
// https://github.com/open-telemetry/opentelemetry-rust/tree/main/opentelemetry-otlp/examples/basic-otlp-http
fn init_tracer_provider() -> anyhow::Result<OpenTelemetryLayer<Registry, sdktrace::Tracer>> {
	let otel_base_config = opentelemetry_otlp::new_pipeline()
		.tracing()
		.with_exporter(opentelemetry_otlp::new_exporter().tonic())
		.with_trace_config(Config::default().with_resource(Resource::new(vec![
			KeyValue::new(
				resource::K8S_CLUSTER_NAME,
				var("NAIS_CLUSTER_NAME").unwrap_or("UNKNOWN_CLUSTER".to_string()),
			),
			KeyValue::new(
				resource::K8S_NAMESPACE_NAME,
				var("NAIS_NAMESPACE").unwrap_or("UNKNOWN_NAMESPACE".to_string()),
			),
			KeyValue::new(
				resource::K8S_DEPLOYMENT_NAME,
				var("NAIS_APP_NAME").unwrap_or("UNKNOWN_DEPLOYMENT".to_string()),
			),
			KeyValue::new(resource::SERVICE_NAME, env!("CARGO_BIN_NAME").to_string()),
		])))
		.install_batch(runtime::Tokio)?
		.tracer(env!("CARGO_BIN_NAME"));
	Ok(tracing_opentelemetry::layer().with_tracer(otel_base_config))
}

pub fn configure_logging() -> anyhow::Result<impl Subscriber> {
	// TODO: Make this configurable instead of hardcoded
	let log_level = LevelFilter::INFO;

	let (plain_log_format, json_log_format) = if std::io::stdout().is_terminal() {
		(Some(fmt::layer().pretty().with_filter(log_level)), None)
	} else {
		(
			None,
			Some(
				fmt::layer()
					.json()
					.flatten_event(true)
					.with_filter(log_level),
			),
		)
	};

	Ok(Registry::default()
		.with(plain_log_format)
		.with(json_log_format)
		.with(init_tracer_provider()?)
		.with(
			filter::Targets::new()
				.with_default(log_level)
				.with_target("axum::rejection", Level::TRACE)
				.with_target("hyper", Level::ERROR)
				.with_target("hyper_util", Level::ERROR)
				.with_target("reqwest", Level::WARN)
				.with_target("tower", Level::ERROR),
		))
}
