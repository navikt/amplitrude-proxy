use async_trait::async_trait;
use bytes::Bytes;
use http::Uri;
use pingora::http::ResponseHeader;
use pingora::ErrorType as ErrType;
use pingora::{
	http::RequestHeader,
	prelude::HttpPeer,
	proxy::{ProxyHttp, Session},
	Error, OrErr, Result,
};
use redact::redact_uri;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use tokio::time;
use tracing::{error, info, trace, warn};
mod annotate;
mod redact;
mod route;
use isbot::Bots;
use std::time::Duration;
use url::Url;

use crate::config::Config;
use crate::errors::{AmplitrudeProxyError, ErrorDescription};
use crate::k8s::{
	self,
	cache::{self, INITIALIZED},
};
use crate::metrics::{
	HANDLED_REQUESTS, INCOMING_REQUESTS, INVALID_PEER, PROXY_ERRORS, REQUEST_DURATION,
	UPSTREAM_200, UPSTREAM_400, UPSTREAM_500,
};
pub struct AmplitudeProxy {
	pub conf: Config,
	pub bots: Bots,
}

impl AmplitudeProxy {
	pub const fn new(conf: Config, bots: Bots) -> Self {
		Self { conf, bots }
	}
}

#[derive(Debug)]
pub struct Location {
	city: String,
	country: String,
}

#[derive(Debug)]
pub struct Ctx {
	request_body_buffer: Vec<u8>,
	response_body_buffer: Vec<u8>,
	route: route::Route,
	location: Option<Location>,
	host: String,
	proxy_start: Option<time::Instant>,
}

#[async_trait]
impl ProxyHttp for AmplitudeProxy {
	type CTX = Ctx;
	fn new_ctx(&self) -> Self::CTX {
		Ctx {
			request_body_buffer: Vec::new(),
			response_body_buffer: Vec::new(),
			route: route::Route::Unexpected(String::new()),
			location: None,
			host: String::new(),
			proxy_start: None,
		}
	}

	/// Request_filter runs before anything else. We can for example set the peer here, through ctx
	/// also Blocks user-agent strings that match known bots
	async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool>
	where
		Self::CTX: Send + Sync,
	{
		ctx.proxy_start = Some(std::time::Instant::now().into());
		session.enable_retry_buffering();
		INCOMING_REQUESTS.inc();
		if !INITIALIZED.load(Ordering::Relaxed) {
			// We only ever want to spawn this thread once. It reads all ingresses once and then sits
			// around watching changes to ingresses
			if INITIALIZED
				.compare_exchange(
					false,
					true,
					Ordering::SeqCst, // sequenctially consistent
					Ordering::Relaxed,
				)
				.is_ok()
			// This is double checked locking, if you squint.
			// https://en.wikipedia.org/wiki/Double-checked_locking
			{
				// This should have a gauge to show that we only ever have one (or zero ) of these
				tokio::spawn(async {
					let e1 = k8s::populate_cache();
					warn!("populating cache: {:?}", e1.await);
					let _e2 = k8s::run_watcher().await;
				});
			}
		}

		let origin = session.downstream_session.get_header("origin").map_or_else(
			|| String::from("missing origin"),
			|x| {
				x.to_str()
					.map_or(String::new(), std::borrow::ToOwned::to_owned)
			},
		);

		ctx.host =
			(*origin.split("//").collect::<Vec<_>>().last().expect(
				"HTTP requests are expected to contain an `origin` header w/scheme specified",
			))
			.to_owned();

		let city = session
			.downstream_session
			.get_header("X-CLIENT-CITY")
			.map_or_else(
				|| {
					String::from("Missing city header, this should not happen, the GCP loadbalancer adds these",)
				},
				|x| {
					x.to_str()
						.map_or(String::new(), std::borrow::ToOwned::to_owned)
				},
			);

		let country = session
			.downstream_session
			.get_header("X-CLIENT-REGION")
			.map_or_else(
				|| {
					String::from("Missing country header, this should not happen the GCP loadbalancer adds these")
				},
				|x| {
					x.to_str()
						.map_or(String::new(), std::borrow::ToOwned::to_owned)
				},
			);

		ctx.location = Some(Location { city, country });

		let owned_parts = session.downstream_session.req_header().as_owned_parts();
		let path = owned_parts.uri.path();
		ctx.route = route::match_route(path.into());

		let user_agent = session.downstream_session.get_header("USER-AGENT").cloned();
		match user_agent {
			Some(ua) => match ua.to_str() {
				Ok(ua) => {
					let bot = self.bots.is_bot(ua);

					if bot {
						session.respond_error(403).await?;
						return Ok(bot);
					}
					Ok(false)
				},
				Err(e) => {
					error!("Err: {e}");
					return Ok(false);
				},
			},
			None => Ok(false),
		}
	}
	// This guy should be the upstream host, all requests through the proxy gets sent th upstream_peer
	async fn upstream_peer(
		&self,
		_session: &mut Session,
		_ctx: &mut Self::CTX,
	) -> Result<Box<HttpPeer>> {
		let mut peer = Box::new(HttpPeer::new(
			format!(
				"{}:{}",
				self.conf.upstream_amplitude.host, self.conf.upstream_amplitude.port
			)
			.to_socket_addrs()
			.expect("Amplitude specified `host` & `port` should give valid `std::net::SocketAddr`")
			.next()
			.expect("SocketAddr should resolve to at least 1 IP address"),
			self.conf.upstream_amplitude.sni.is_some(),
			self.conf.upstream_amplitude.sni.clone().unwrap_or_default(),
		));

		peer.options.tcp_keepalive = Some(pingora::protocols::TcpKeepalive {
			idle: std::time::Duration::from_secs(120),
			interval: std::time::Duration::from_secs(5),
			count: 3,
		});
		Ok(peer)
	}

	async fn request_body_filter(
		&self,
		session: &mut Session,
		body: &mut Option<Bytes>,
		end_of_stream: bool,
		ctx: &mut Self::CTX,
	) -> Result<()>
	where
		Self::CTX: Send + Sync,
	{
		// buffer the data
		if let Some(b) = body {
			ctx.request_body_buffer.extend(&b[..]);
			// drop the body - we've consumed it as b
			b.clear();
		}
		if end_of_stream {
			// This is the last chunk, we can process the data now
			if !ctx.request_body_buffer.is_empty() {
				let content_type = session
					.downstream_session
					.get_header("content-type")
					.map_or_else(
						|| String::from("no content type header"),
						|x| {
							x.to_str()
								.map_or(String::new(), std::borrow::ToOwned::to_owned)
						},
					);

				// We should do content negotiation, apparently
				// This must be a downsteam misconfiguration, surely??
				let mut json: Value = if content_type
					.to_lowercase()
					.contains("application/x-www-form-urlencoded")
				{
					parse_url_encoded(&String::from_utf8_lossy(&ctx.request_body_buffer))?
				} else {
					serde_json::from_slice(&ctx.request_body_buffer)
						.or_err(
							pingora::ErrorType::Custom(
								AmplitrudeProxyError::RequestContainsInvalidJson.into(),
							),
							"Failed to parse request body",
						)
						.map_err(|e| *e)?
				};

				annotate::with_proxy_version(
					&mut json,
					&format!("{}-{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
				);

				//				annotate::with_hostname(&mut json, ctx.host.as_ref());
				redact::traverse_and_redact(&mut json);

				annotate_with_nav_extras(&self.conf, &mut json, &ctx);
				// This uses exactly "event_properties, which maybe only amplitude has"
				if let Some(loc) = &ctx.location {
					annotate::with_location(&mut json, &loc.city, &loc.country);
				}

				// Surely there is a correct-by-conctruction value type that can be turned into a string without fail
				if let Ok(json_body) = serde_json::to_string(&json) {
					*body = Some(Bytes::from(json_body));
				} else {
					// Technically, we do a bunch of mut Value, so there is
					// A gurantee from the type system that this never happens
					// however, we cant produce a witness to this so here we are.
					return Err(Error::explain(
						pingora::ErrorType::Custom(AmplitrudeProxyError::JsonCoParseError.into()),
						"failed to co-parse request body",
					));
				}
			}
		}
		Ok(())
	}

	async fn response_filter(
		&self,
		session: &mut Session,
		upstream_response: &mut ResponseHeader,
		ctx: &mut Self::CTX,
	) -> Result<()>
	where
		Self::CTX: Send + Sync,
	{
		match upstream_response.status.as_u16() {
			500 => UPSTREAM_500.inc(),
			400 => {
				UPSTREAM_400.inc();
			},
			200 => {
				UPSTREAM_200.inc();
				trace!(
					"status: {}, reason {:?}, {} - Origin: {}",
					upstream_response.status,
					upstream_response.get_reason_phrase(),
					session.request_summary(),
					ctx.host
				);
			},

			_ => {},
		}
		Ok(())
	}

	fn response_body_filter(
		&self,
		session: &mut Session,
		body: &mut Option<Bytes>,
		end_of_stream: bool,
		ctx: &mut Self::CTX,
	) -> Result<Option<Duration>, Box<Error>>
	where
		Self::CTX: Send + Sync,
	{
		// buffer the data
		if let Some(b) = body {
			ctx.response_body_buffer.extend(&b[..]);
			// drop the body - we've consumed it as b
			b.clear();
		}
		if end_of_stream {
			*body = Some(Bytes::copy_from_slice(&ctx.response_body_buffer));
		}
		Ok(None)
	}
	async fn upstream_request_filter(
		&self,
		_session: &mut Session,
		upstream_request: &mut RequestHeader,
		ctx: &mut Self::CTX,
	) -> Result<()> {
		// It's hard to know how big the body is before we start touching it
		// We work around that by removing content length and setting the
		// transfer encoding as chunked. The source code in pingora core looks like it would
		// do it automatically, but I don't see it happening, hence the explicit bits here
		upstream_request.remove_header("Content-Length");
		upstream_request
			.insert_header("Transfer-Encoding", "Chunked")
			.expect("Needs correct transfer-encoding scheme header set");

		match &ctx.route {
			// TODO: These are hardcoded, it's mostly fine
			route::Route::Amplitude(_) | route::Route::AmplitudeCollect(_) => {
				upstream_request
					.insert_header("Host", "api.eu.amplitude.com")
					.expect("Needs correct Host header");
				upstream_request.set_uri(Uri::from_static("/2/httpapi"));
			},
			route::Route::Unexpected(_) => {},
		}

		Ok(())
	}

	async fn logging(&self, session: &mut Session, e: Option<&Error>, ctx: &mut Self::CTX)
	where
		Self::CTX: Send + Sync,
	{
		let elapsed = ctx
			.proxy_start
			.map(|instant| instant.elapsed().as_secs_f64())
			.unwrap_or(Duration::ZERO.as_secs_f64());

		REQUEST_DURATION.observe(elapsed);
		let Some(err) = e else {
			// happy path
			HANDLED_REQUESTS.inc();
			trace!("Handled request: {}", session.request_summary());
			return;
		};

		// Some error happened
		let error = if let ErrType::Custom(error_description) = err.etype {
			// First check for error causes we've written ourselves
			AmplitrudeProxyError::from_str(error_description)
				.map(|amplitude_proxy_error| match amplitude_proxy_error {
					AmplitrudeProxyError::NoMatchingPeer => {
						INVALID_PEER.inc();
						ErrorDescription::AmplitrudeProxyError(amplitude_proxy_error)
					},
					// For the `::Custom` pingora errorse we track and don't have an explicit metric for
					error => ErrorDescription::AmplitrudeProxyError(error),
				})
				.unwrap_or(ErrorDescription::UntrackedError)
		} else {
			// Then check for the pingora errors we're keen to track
			match err.etype {
				ErrType::TLSHandshakeFailure
				| ErrType::TLSHandshakeTimedout
				| ErrType::InvalidCert
				| ErrType::HandshakeError => ErrorDescription::SslError,

				ErrType::ConnectTimedout
				| ErrType::ConnectRefused
				| ErrType::ConnectNoRoute
				| ErrType::ConnectError
				| ErrType::BindError
				| ErrType::AcceptError
				| ErrType::SocketError => ErrorDescription::ConnectionError,

				ErrType::ConnectionClosed => ErrorDescription::ClientDisconnectedError,
				ErrType::ConnectProxyFailure => ErrorDescription::UpstreamConnectionFailure,

				// All the rest are ignored for now, bring in when needed
				_ => ErrorDescription::UntrackedError,
			}
		};
		let untracked = if error == ErrorDescription::UntrackedError {
			"Untracked error: "
		} else {
			""
		};
		error!("{untracked}{}: {:?}", session.request_summary(), err);
		PROXY_ERRORS.with_label_values(&[(error.as_str())]).inc();
	}
}
fn parse_url_encoded(data: &str) -> Result<Value, pingora::Error> {
	let parsed: HashMap<String, String> = serde_urlencoded::from_str(data)
		.explain_err(
			pingora::ErrorType::Custom(AmplitrudeProxyError::RequestContainsInvalidJson.into()),
			|e| format!("urlencoded json malformed: {e}"),
		)
		.map_err(|e| *e)?;

	let client = parsed.get("client").cloned();
	let events_data = parsed.get("e").map_or(json!(null), |e| {
		serde_json::from_str::<Value>(e).unwrap_or(json!(null))
	});

	Ok(json!({ "events": events_data, "api_key": client }))
}
fn annotate_with_nav_extras(conf: &Config, json: &mut Value, ctx: &Ctx) {
	let platform = get_source_name(json)
		.or_else(|| get_platform(json))
		.and_then(|s| s.parse::<Uri>().ok());

	if platform.is_some() {
		annotate::with_urls(json, &ctx.host);
	}

	let platform_str = platform
		.map(|x| x.to_string())
		.unwrap_or_else(|| "web".to_string());

	if let Some(app) = cache::get_app_info_with_longest_prefix(&platform_str) {
		annotate::with_app_info(json, &app, &ctx.host, "prod-gcp");
	}

	// The priority is, i think:
	// 1. "client" field if it exists, this is new clients, {e: [], client:}..
	// 2. "api_key" field if it exists and is not "default", this is collect-auto, sometimes
	// 3. Fall back to environment-based key, this is the wtfbbq case
	let key = if let Some(client) = json.get("client").and_then(Value::as_str) {
		client.to_string()
	} else if let Some(api_key) = json.get("api_key").and_then(Value::as_str) {
		if api_key != "default" {
			api_key.to_string()
		} else {
			let is_dev = ctx.host.contains("dev.nav.no") || ctx.host.contains("localhost");
			if is_dev {
				conf.amplitude_api_key_dev.clone()
			} else {
				conf.amplitude_api_key_prod.clone()
			}
		}
	} else {
		let is_dev = ctx.host.contains("dev.nav.no") || ctx.host.contains("localhost");
		if is_dev {
			conf.amplitude_api_key_dev.clone()
		} else {
			conf.amplitude_api_key_prod.clone()
		}
	};

	annotate::with_key(json, key);
}

fn get_platform(value: &Value) -> Option<String> {
	value
		.get("events")
		.and_then(|v| v.as_array())
		.and_then(|events| {
			events.iter().find_map(|event| {
				event
					.get("platform")
					.and_then(|v| v.as_str())
					.map(String::from)
			})
		})
}

fn get_source_name(value: &Value) -> Option<String> {
	value
		.get("events")
		.and_then(|v| v.as_array())
		.and_then(|events| {
			events.iter().find_map(|event| {
				event
					.get("ingestion_metadata")
					.and_then(|metadata| metadata.get("source_name"))
					.and_then(|v| v.as_str())
					.and_then(|source_name| Url::parse(source_name).ok())
					.map(|url| {
						// This here is a poor man normalization.
						let scheme = url.scheme();
						let host = url.host_str().unwrap_or("");
						let path = url.path();
						format!("{}://{}{}", scheme, host, path)
					})
			})
		})
}

fn categorize_other_environment(host: String, environments: &[String]) -> String {
	if environments.iter().any(|env| host.ends_with(env)) {
		"dev".into()
	} else if host.contains("localhost") {
		"localhost".into()
	} else {
		"other".into()
	}
}

fn is_using_new_sdk(event: &Value) -> bool {
	if let Some(body) = event.get("body") {
		if body.get("events").is_some() && body.get("api_key").is_some() {
			return true;
		} else if body.get("e").is_some() && body.get("client").is_some() {
			return false;
		}
	}
	false
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::{json, Value};

	#[test]
	fn test_categorize_environment_dev() {
		let environments = ["dev.nav.no", "fooo"].map(|e| e.into());
		assert_eq!(
			categorize_other_environment("example.dev.nav.no".into(), &environments),
			"dev"
		);
	}

	#[test]
	fn test_categorize_environment_localhost() {
		let environments = ["dev", "staging"].map(|e| e.into());
		assert_eq!(
			categorize_other_environment("localhost".into(), &environments),
			"localhost"
		);
		assert_eq!(
			categorize_other_environment("mylocalhost.com".into(), &environments),
			"localhost"
		);
	}

	#[test]
	fn test_categorize_environment_other() {
		let environments = ["dev", "staging"].map(|e| e.into());

		assert_eq!(
			categorize_other_environment("example.com".into(), &environments),
			"other"
		);
	}

	#[test]
	fn test_parse_url_encoded() {
		let input = "e=%5B%7B%22device_id%22%3A%22xPrSCZPag7CI1n6cHHrIPn%22%2C%22user_id%22%3Anull%2C%22timestamp%22%3A1728375957190%2C%22event_id%22%3A806%2C%22session_id%22%3A1728375927004%2C%22event_type%22%3A%22bes%C3%B8k%22%2C%22version_name%22%3Anull%2C%22platform%22%3A%22https%3A%2F%2Fwww.nav.no%2F%22%2C%22os_name%22%3A%22Chrome%22%2C%22os_version%22%3A%22129%22%2C%22device_model%22%3A%22Macintosh%22%2C%22device_manufacturer%22%3A%22Apple%22%2C%22language%22%3A%22en-GB%22%2C%22api_properties%22%3A%7B%7D%2C%22event_properties%22%3A%7B%22sidetittel%22%3A%22Forside%20privatperson%20-%20nav.no%22%2C%22innlogging%22%3Afalse%2C%22parametre%22%3A%7B%22context%22%3A%22privatperson%22%2C%22simple%22%3Afalse%2C%22simpleHeader%22%3Afalse%2C%22redirectToApp%22%3Afalse%2C%22level%22%3A%22Level3%22%2C%22language%22%3A%22nb%22%2C%22availableLanguages%22%3A%5B%22en%22%2C%22nb%22%5D%2C%22breadcrumbs%22%3A%5B%5D%2C%22utilsBackground%22%3A%22white%22%2C%22feedback%22%3Afalse%2C%22chatbot%22%3Atrue%2C%22chatbotVisible%22%3Afalse%2C%22shareScreen%22%3Atrue%2C%22maskHotjar%22%3Afalse%2C%22logoutWarning%22%3Atrue%2C%22BREADCRUMBS%22%3Afalse%7D%2C%22platform%22%3A%22https%3A%2F%2Fwww.nav.no%2F%22%2C%22origin%22%3A%22decorator-next%22%2C%22originVersion%22%3A%22unknown%22%2C%22viaDekoratoren%22%3Atrue%2C%22fromNext%22%3Atrue%7D%2C%22user_properties%22%3A%7B%7D%2C%22uuid%22%3A%2201056959-37fe-4021-94a4-6b08c6238913%22%2C%22library%22%3A%7B%22name%22%3A%22amplitude-js%22%2C%22version%22%3A%228.21.9%22%7D%2C%22sequence_number%22%3A862%2C%22groups%22%3A%7B%7D%2C%22group_properties%22%3A%7B%7D%2C%22user_agent%22%3A%22Mozilla%2F5.0%20%28Macintosh%3B%20Intel%20Mac%20OS%20X%2010_15_7%29%20AppleWebKit%2F537.36%20%28KHTML%2C%20like%20Gecko%29%20Chrome%2F129.0.0.0%20Safari%2F537.36%22%2C%22partner_id%22%3Anull%7D%5D";
		let expected: Value = json!({
				"api_key": null,
			"events": [{
				"device_id": "xPrSCZPag7CI1n6cHHrIPn",
				"user_id": null,
				"timestamp": 1728375957190_i64,
				"event_id": 806,
				"session_id": 1728375927004_i64,
				"event_type": "besøk",
				"version_name": null,
				"platform": "https://www.nav.no/",
				"os_name": "Chrome",
				"os_version": "129",
				"device_model": "Macintosh",
				"device_manufacturer": "Apple",
				"language": "en-GB",
				"api_properties": {},
				"event_properties": {
					"sidetittel": "Forside privatperson - nav.no",
					"innlogging": false,
					"parametre": {
						"context": "privatperson",
						"simple": false,
						"simpleHeader": false,
						"redirectToApp": false,
						"level": "Level3",
						"language": "nb",
						"availableLanguages": ["en", "nb"],
						"breadcrumbs": [],
						"utilsBackground": "white",
						"feedback": false,
						"chatbot": true,
						"chatbotVisible": false,
						"shareScreen": true,
						"maskHotjar": false,
						"logoutWarning": true,
						"BREADCRUMBS": false
					},
					"platform": "https://www.nav.no/",
					"origin": "decorator-next",
					"originVersion": "unknown",
					"viaDekoratoren": true,
					"fromNext": true
				},
				"user_properties": {},
				"uuid": "01056959-37fe-4021-94a4-6b08c6238913",
				"library": {
					"name": "amplitude-js",
					"version": "8.21.9"
				},
				"sequence_number": 862,
				"groups": {},
				"group_properties": {},
				"user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36",
				"partner_id": null
			}]
		});

		let parsed = parse_url_encoded(input).expect("Failed to parse");
		assert_eq!(parsed, expected);
	}
}
