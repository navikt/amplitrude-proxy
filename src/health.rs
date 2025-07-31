use async_trait::async_trait;
use pingora::{
	Result,
	prelude::HttpPeer,
	proxy::{ProxyHttp, Session},
};
use tracing::trace;

pub struct Probes;

#[derive(Debug)]
pub struct Ctx {}

#[async_trait]
impl ProxyHttp for Probes {
	type CTX = Ctx;
	fn new_ctx(&self) -> Self::CTX {
		Ctx {}
	}

	/// Handle the incoming request.
	///
	/// In this phase, users can parse, validate, rate limit, perform access control and/or
	/// return a response for this request.
	///
	/// If the user already sent a response to this request, an `Ok(true)` should be returned so that
	/// the proxy would exit. The proxy continues to the next phases when `Ok(false)` is returned.
	///
	/// By default this filter does nothing and returns `Ok(false)`.
	async fn request_filter(&self, session: &mut Session, _ctx: &mut Self::CTX) -> Result<bool>
	where
		Self::CTX: Send + Sync,
	{
		if session
			.downstream_session
			.req_header()
			.as_owned_parts()
			.uri
			.path()
			.contains("is_alive")
		// this also matches is_aliveeeeeeeee etc
		{
			session.respond_error(200).await?; // Can we respond without saying error?
			trace!("is_alive: 200");
			return Ok(true);
		}
		session.respond_error(404).await?;
		trace!("fail: 404");

		Ok(true) //exit, do nothing else. We're done
	}

	/// After request_filter, this guy gets called but we've already handled everything so it does nothing.
	async fn upstream_peer(
		&self,
		_session: &mut Session,
		_ctx: &mut Self::CTX,
	) -> Result<Box<HttpPeer>> {
		// Panic is not the best implementation here.
		// The correct implementation would be to have the readyness probe go all the way down here and
		// check that we can connect to the upstream. In case of ip change
		panic!(); // We don't need an implementation here as going further from request_filter is a bug in this proxy
	}
}
