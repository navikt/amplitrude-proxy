use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::net::ToSocketAddrs;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use bytes::Bytes;
use http::Uri;
use pingora::http::ResponseHeader;
use pingora::ErrorType as ErrType;
use pingora::{
	http::RequestHeader,
	prelude::HttpPeer,
	proxy::{ProxyHttp, Session},
	Result,
};
use pingora::{Error, OrErr};
use serde_json::{json, Value};
use strum::IntoStaticStr;
use tracing::{error, info, warn};
mod annotate;
mod redact;
mod route;
use isbot::Bots;
use serde_urlencoded;

use crate::config::Config;
use crate::k8s;
use crate::k8s::cache::{self, INITIALIZED};
use crate::metrics::{
	AMPLITUDE_PEER, BODY_PARSE_ERROR, CONNECTION_ERRORS, HANDLED_REQUESTS, INCOMING_REQUESTS,
	INVALID_PEER, PROXY_ERRORS, REDACTED_BODY_COPARSE_ERROR, SSL_ERROR, UMAMI_PEER,
	UPSTREAM_CONNECTION_FAILURES, UPSTREAM_PEER,
};

pub struct AmplitudeProxy {
	pub conf: Config,
	pub addr: std::net::SocketAddr,
	pub sni: Option<String>,
	pub bots: Bots,
}

impl AmplitudeProxy {
	pub fn new(
		conf: Config,
		addr: std::net::SocketAddr,
		sni: Option<String>,
		bots: Bots,
	) -> AmplitudeProxy {
		AmplitudeProxy {
			conf,
			addr,
			sni,
			bots,
		}
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
	ingress: String,
}

#[derive(Debug, IntoStaticStr)]
enum AmplitudProxyError {
	JsonParseError,
	JsonCoParseError,
}

impl Display for AmplitudProxyError {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{:?}", self)
	}
}

#[async_trait]
impl ProxyHttp for AmplitudeProxy {
	type CTX = Ctx;
	fn new_ctx(&self) -> Self::CTX {
		Ctx {
			request_body_buffer: Vec::new(),
			response_body_buffer: Vec::new(),
			route: route::Route::Unexpected("".into()),
			location: None,
			ingress: "".into(),
		}
	}

	/// Request_filter runs before anything else. We can for example set the peer here, through ctx
	/// also Blocks user-agent strings that match known bots
	async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool>
	where
		Self::CTX: Send + Sync,
	{
		INCOMING_REQUESTS.inc();
		if !INITIALIZED.load(Ordering::Relaxed) {
			// We only ever want to spawn this thread once. It reads all ingresses once and then sits
			// around watching changes to ingresses
			if let Ok(_) =
				// This is double checked locking, if you squint.
				// https://en.wikipedia.org/wiki/Double-checked_locking
				INITIALIZED.compare_exchange(
					false,
					true,
					Ordering::SeqCst, // sequenctially consistent
					Ordering::Relaxed,
				) {
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

		ctx.ingress = origin
			.split("//")
			.collect::<Vec<_>>()
			.last()
			.unwrap()
			.to_string();

		let city = session
			.downstream_session
			.get_header("x-client-city")
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
			.get_header("x-client-region")
			.map_or_else(
				|| {
					String::from("Missing country header, this should not happen the GCP loadbalancer adds these")
				},
				|x| {
					x.to_str()
						.map_or(String::from(""), std::borrow::ToOwned::to_owned)
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
						// ^ This respond_error bit is silly, surely we can just respond?
						info!("This request's UA matches a known bot:\n\t{ua}");
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
		ctx: &mut Self::CTX,
	) -> Result<Box<HttpPeer>> {
		UPSTREAM_PEER
			.with_label_values(&[&format!("{}", &ctx.route)])
			.inc();
		match &ctx.route {
			route::Route::Umami(_) => {
				UMAMI_PEER.inc();
				Ok(Box::new(HttpPeer::new(
					format!(
						"{}:{}",
						self.conf.upstream_umami.host, self.conf.upstream_umami.port
					)
					.to_socket_addrs()
					.expect(
						"Umami specified `host` & `port` should give valid `std::net::SocketAddr`",
					)
					.next()
					.expect("SocketAddr should resolve to at least 1 IP address"),
					self.conf.upstream_umami.sni.is_some(),
					self.conf
						.upstream_umami
						.sni
						.clone()
						.unwrap_or_else(|| "".into()),
				)))
			},
			route::Route::Amplitude(_) | route::Route::AmplitudeCollect(_) => {
				AMPLITUDE_PEER.inc();
				Ok(Box::new(HttpPeer::new(
				format!(
					"{}:{}",
					self.conf.upstream_amplitude.host, self.conf.upstream_amplitude.port
				)
				.to_socket_addrs()
				.expect(
					"Amplitude specified `host` & `port` should give valid `std::net::SocketAddr`",
				)
				.next()
				.expect("SocketAddr should resolve to at least 1 IP address"),
				self.conf.upstream_amplitude.sni.is_some(),
				self.conf
					.upstream_amplitude
					.sni
					.clone()
					.unwrap_or_else(|| "".into()),
			)))
			},
			route::Route::Unexpected(s) => {
				INVALID_PEER.inc();
				let error = format!("creating peer: {}", s);
				Err(Error::explain(
					pingora::ErrorType::Custom("no matching peer for path"),
					error,
				))
			},
		}
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
		let content_type = session
			.downstream_session
			.get_header("content-type")
			.map_or_else(
				|| String::from("no content type header"),
				|x| {
					x.to_str()
						.map_or(String::from(""), std::borrow::ToOwned::to_owned)
				},
			);

		// buffer the data
		if let Some(b) = body {
			ctx.request_body_buffer.extend(&b[..]);
			// drop the body - we've consumed it as b
			b.clear();
		}
		if end_of_stream {
			// This is the last chunk, we can process the data now
			if !ctx.request_body_buffer.is_empty() {
				// We should do proper content negotiation, apparently
				let json: Result<serde_json::Value, _>;
				if content_type == "application/x-www-form-urlencoded; charset=UTF-8" {
					json = parse_url_encoded(&String::from_utf8_lossy(&ctx.request_body_buffer));
				} else {
					json = serde_json::from_slice(&ctx.request_body_buffer)
				}
				// this erros on "use of a moved value"
				let Ok(mut v) = json else {
					return {
						BODY_PARSE_ERROR.inc();
						Err(Error::explain(
							pingora::ErrorType::Custom("invalid request-json"),
							"Failed to parse request body",
						))
					};
				};

				let platform = get_platform(&mut v);
				redact::traverse_and_redact(&mut v);
				annotate::annotate_with_proxy_version(&mut v, "amplitrude-1.0.0");

				info!("platform: {:?}", platform);
				if let Some(app) =
					cache::get_app_info_with_longest_prefix(platform.unwrap_or("".into()))
				{
					annotate::annotate_with_app_info(&mut v, &app, &ctx.ingress);
					info!("Found app: {:?}", app);
					annotate::annotate_with_prod(&mut v, self.conf.amplitude_api_key_prod.clone());
				}

				// This uses exactly "event_properties, which maybe only amplitude has"
				if let Some(loc) = &ctx.location {
					annotate::annotate_with_location(&mut v, &loc.city, &loc.country);
				}

				// Surely there is a correct-by-conctruction Value type that can be turned into a string without fail
				serde_json::to_string(&v)
					.map_err(|e| {
						// Technically, we do a bunch of mut Value, so there is
						// A gurantee from the type system that this never happens
						// however, we cant produce a witness to this so here we are.
						REDACTED_BODY_COPARSE_ERROR.inc();
						e
					})
					.or_err(
						pingora::ErrorType::Custom(AmplitudProxyError::JsonCoParseError.into()),
						"Failed to co-parse redacted request body",
					)
					.map(|json_body| {
						*body = Some(Bytes::from(json_body));
					})?;
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
		info!(
			"status: {}, reason {:?}, {} - Origin: {}",
			upstream_response.status,
			upstream_response.get_reason_phrase(),
			session.request_summary(),
			ctx.ingress
		);
		Ok(())
	}
	fn upstream_response_body_filter(
		&self,
		session: &mut Session,
		body: &mut Option<Bytes>,
		end_of_stream: bool,
		ctx: &mut Self::CTX,
	) -> () {
		if let Some(b) = body {
			ctx.response_body_buffer.extend(&b[..]);
			// drop the body - we've consumed it as b
			b.clear();
		}
		if end_of_stream {
			info!(
				"{} - {}",
				session.request_summary(),
				String::from_utf8_lossy(&ctx.response_body_buffer)
			);
		}
	}

	/// Redact path and query parameters of request
	/// TODO: Also ensure that path fragments are redacted?
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
			.unwrap();

		match &ctx.route {
			route::Route::Umami(_) => {
				upstream_request
					.insert_header("Host", "umami.nav.no")
					.expect("Needs correct Host header");

				// We are using vercel headers here because Umami supports them
				// and they are not configurable. We already have this info in the request
				// as x-client-city, x-client-countrlly but umami does not support those names.
				// (umami also supports Cloudflare headers, which we aren't (but could be) using )
				if let Some(loc) = &ctx.location {
					upstream_request
						.insert_header("X-Vercel-IP-Country", &loc.country)
						.unwrap();

					upstream_request
						.insert_header("X-Vercel-City", &loc.city)
						.unwrap();
				}
				upstream_request
					.insert_header("Host", "api.eu.amplitude.com")
					.expect("Needs correct Host header");
			},
			route::Route::Amplitude(_) | route::Route::AmplitudeCollect(_) => {
				upstream_request
					.insert_header("Host", "api.eu.amplitude.com")
					.expect("Needs correct Host header");
			},
			route::Route::Unexpected(_) => {},
		}

		match &ctx.route {
			route::Route::Umami(_) => {
				upstream_request.set_uri(Uri::from_static("/api/send"));
			},
			route::Route::Amplitude(_) => {
				upstream_request.set_uri(Uri::from_static("/2/httpapi"));
			},
			route::Route::AmplitudeCollect(_) => {
				upstream_request.set_uri(Uri::from_static("/2/httpapi"));
			},
			route::Route::Unexpected(_) => {},
		}

		Ok(())
	}

	async fn logging(&self, session: &mut Session, e: Option<&Error>, _ctx: &mut Self::CTX)
	where
		Self::CTX: Send + Sync,
	{
		let Some(err) = e else {
			// happy path
			HANDLED_REQUESTS.inc();
			info!("Handled request: {}", session.request_summary());
			return;
		};

		/// Collection of errors we are interested in tracking w/metrics
		#[derive(Debug)]
		enum ErrorDescription {
			SslError,
			ConnectionError,
			UpstreamConnectionFailure,
			UntrackedError,
			AmplitudeProxyError(AmplitudProxyError),
		}
		impl Display for ErrorDescription {
			fn fmt(&self, f: &mut Formatter) -> fmt::Result {
				write!(f, "{:?}", self)
			}
		}

		// Some error happened
		error!("{}: {:?}", session.request_summary(), err);
		let pingora_error = match err.etype {
			ErrType::TLSHandshakeFailure
			| ErrType::TLSHandshakeTimedout
			| ErrType::InvalidCert
			| ErrType::HandshakeError => {
				SSL_ERROR.inc();
				Some(ErrorDescription::SslError)
			},

			ErrType::ConnectTimedout
			| ErrType::ConnectRefused
			| ErrType::ConnectNoRoute
			| ErrType::ConnectError
			| ErrType::BindError
			| ErrType::AcceptError
			| ErrType::ConnectionClosed
			| ErrType::SocketError => {
				CONNECTION_ERRORS.inc();
				Some(ErrorDescription::ConnectionError)
			},

			ErrType::ConnectProxyFailure => {
				UPSTREAM_CONNECTION_FAILURES.inc();
				Some(ErrorDescription::UpstreamConnectionFailure)
			},

			// All the rest are ignored for now, bring in when needed
			_ => None,
		};

		// Check to see if we've wrapped a different error
		let error_description = pingora_error.unwrap_or(match err.context {
			Some(error_string) => match error_string {
				AmplitudProxyError::JsonCoParseError => todo!(),
				_ => ErrorDescription::UntrackedError,
			},
			None => ErrorDescription::UntrackedError,
		});
		PROXY_ERRORS
			.with_label_values(&[&error_description.to_string()])
			.inc();
	}
}

fn parse_url_encoded(data: &str) -> Result<Value, serde_json::Error> {
	let parsed: HashMap<String, String> = serde_urlencoded::from_str(data).unwrap();
	let client = parsed.get("client").cloned();

	let events_data = match parsed.get("e") {
		Some(e) => serde_json::from_str::<Value>(e).unwrap_or(json!(null)),
		None => json!(null),
	};

	Ok(json!({ "events": events_data, "api-key": client }))
}

fn get_platform(value: &Value) -> Option<String> {
	if let Some(events) = value.get("events").and_then(|v| v.as_array()) {
		events
			.iter()
			.filter_map(|event| {
				event
					.get("platform")
					.and_then(|v| v.as_str())
					.map(String::from)
			})
			.next()
	} else {
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::{json, Value};

	#[test]
	fn test_parse_url_encoded() {
		let input = "e=%5B%7B%22device_id%22%3A%22xPrSCZPag7CI1n6cHHrIPn%22%2C%22user_id%22%3Anull%2C%22timestamp%22%3A1728375957190%2C%22event_id%22%3A806%2C%22session_id%22%3A1728375927004%2C%22event_type%22%3A%22bes%C3%B8k%22%2C%22version_name%22%3Anull%2C%22platform%22%3A%22https%3A%2F%2Fwww.nav.no%2F%22%2C%22os_name%22%3A%22Chrome%22%2C%22os_version%22%3A%22129%22%2C%22device_model%22%3A%22Macintosh%22%2C%22device_manufacturer%22%3A%22Apple%22%2C%22language%22%3A%22en-GB%22%2C%22api_properties%22%3A%7B%7D%2C%22event_properties%22%3A%7B%22sidetittel%22%3A%22Forside%20privatperson%20-%20nav.no%22%2C%22innlogging%22%3Afalse%2C%22parametre%22%3A%7B%22context%22%3A%22privatperson%22%2C%22simple%22%3Afalse%2C%22simpleHeader%22%3Afalse%2C%22redirectToApp%22%3Afalse%2C%22level%22%3A%22Level3%22%2C%22language%22%3A%22nb%22%2C%22availableLanguages%22%3A%5B%22en%22%2C%22nb%22%5D%2C%22breadcrumbs%22%3A%5B%5D%2C%22utilsBackground%22%3A%22white%22%2C%22feedback%22%3Afalse%2C%22chatbot%22%3Atrue%2C%22chatbotVisible%22%3Afalse%2C%22shareScreen%22%3Atrue%2C%22maskHotjar%22%3Afalse%2C%22logoutWarning%22%3Atrue%2C%22BREADCRUMBS%22%3Afalse%7D%2C%22platform%22%3A%22https%3A%2F%2Fwww.nav.no%2F%22%2C%22origin%22%3A%22decorator-next%22%2C%22originVersion%22%3A%22unknown%22%2C%22viaDekoratoren%22%3Atrue%2C%22fromNext%22%3Atrue%7D%2C%22user_properties%22%3A%7B%7D%2C%22uuid%22%3A%2201056959-37fe-4021-94a4-6b08c6238913%22%2C%22library%22%3A%7B%22name%22%3A%22amplitude-js%22%2C%22version%22%3A%228.21.9%22%7D%2C%22sequence_number%22%3A862%2C%22groups%22%3A%7B%7D%2C%22group_properties%22%3A%7B%7D%2C%22user_agent%22%3A%22Mozilla%2F5.0%20%28Macintosh%3B%20Intel%20Mac%20OS%20X%2010_15_7%29%20AppleWebKit%2F537.36%20%28KHTML%2C%20like%20Gecko%29%20Chrome%2F129.0.0.0%20Safari%2F537.36%22%2C%22partner_id%22%3Anull%7D%5D";
		let expected: Value = json!({
				"api-key": null,
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
