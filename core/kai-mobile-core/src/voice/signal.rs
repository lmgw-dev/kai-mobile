//! The call's signalling over the device link (design §7.2, §7.3, §7.8): the offer's POST, the
//! hangup of a stale call, and the warm connection. Every request goes with the pinned client the
//! link's [`Tls`] builds (the device's certificate, lmgw's CA as the only root), on its shared pool,
//! so the connection [`warm`] opens is the one the POST takes.
//!
//! - **The base URL** is the one that last answered the feed. When none has (a fresh process whose
//!   feed is not up yet), or when the POST cannot connect there (the network changed), the URLs are
//!   raced with the warm-up request ([`race_base`], §5.2's rule): only a connection is raced, never
//!   the POST, so no offer reaches lmgw twice.
//! - **The warm-up** is `GET /v1/realtime/calls`, a path the device listener answers 404 without a
//!   handler: the TCP connection, the TLS 1.3 handshake and its Keystore signature run while Kotlin
//!   builds the offer (§7.2), and nothing on lmgw is read or written.

use std::time::Duration;

use lmgw_client::requests::{
    self, ApiError, Method, Request,
    calls::{self, CallAnswer, HungUp},
};

use crate::link::{failure::Failure, http::NoAnswer, race::race, tls::Tls};

/// Why a request got no answer it could use.
#[derive(Debug, Clone)]
pub enum Fail {
    /// lmgw answered with a refusal (or an answer that does not read).
    Refused { status: u16, error: ApiError },
    /// The connection could not be made: the request was not sent, another URL may be tried.
    Connect(Failure),
    /// No answer within `lmgw.timeout_s`, or the connection broke after the request went.
    NoAnswer(NoAnswer),
}

impl Fail {
    pub fn words(&self, timeout: Duration) -> String {
        match self {
            Self::Refused { status, error } => {
                format!("{} ({}, HTTP {status})", error.message, error.code)
            }
            Self::Connect(f) => format!("lmgw is not reachable: {}", f.words()),
            Self::NoAnswer(n) => n.words(timeout),
        }
    }
}

fn warm_request() -> Request {
    Request {
        method: Method::Get,
        path: "/v1/realtime/calls".to_owned(),
        headers: Vec::new(),
        body: None,
    }
}

fn builder(tls: &Tls, base: &str, req: &Request) -> reqwest::RequestBuilder {
    let http = tls.http();
    let url = req.url(base);
    let mut rb = match req.method {
        Method::Get => http.get(url),
        Method::Post => http.post(url),
    };
    for h in &req.headers {
        rb = rb.header(h.name.as_str(), h.value.as_str());
    }
    if let Some(body) = &req.body {
        rb = rb.body(body.clone());
    }
    rb
}

/// Sends `req` to `base` and reads its status, `Location` and body, each within `timeout`.
async fn exchange(
    tls: &Tls,
    base: &str,
    req: &Request,
    timeout: Duration,
) -> Result<(u16, Option<String>, String), Fail> {
    let resp = match tokio::time::timeout(timeout, builder(tls, base, req).send()).await {
        Err(_) => return Err(Fail::NoAnswer(NoAnswer::Timeout)),
        Ok(Err(e)) if e.is_connect() => return Err(Fail::Connect(Failure::of(&e))),
        Ok(Err(e)) => return Err(Fail::NoAnswer(NoAnswer::Failed(Failure::of(&e)))),
        Ok(Ok(r)) => r,
    };
    let status = resp.status().as_u16();
    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body = tokio::time::timeout(timeout, resp.text())
        .await
        .map_err(|_| Fail::NoAnswer(NoAnswer::Timeout))?
        .map_err(|e| Fail::NoAnswer(NoAnswer::Failed(Failure::of(&e))))?;
    Ok((status, location, body))
}

/// The warm-up (module doc): a connection to `base` in the pool. Its answer is not read.
pub async fn warm(tls: &Tls, base: &str, timeout: Duration) -> Result<(), Fail> {
    exchange(tls, base, &warm_request(), timeout)
        .await
        .map(|(status, ..)| tracing::debug!(%base, status, "voice: the connection is warm"))
}

/// The URL race with the warm-up (module doc): the first URL whose connection answers.
pub async fn race_base(
    tls: &Tls,
    urls: Vec<String>,
    stagger: Duration,
    timeout: Duration,
) -> Result<String, Vec<(String, String)>> {
    let attempt = |base: &str, _answered| {
        let tls = tls.clone();
        let base = base.to_owned();
        async move {
            warm(&tls, &base, timeout)
                .await
                .map_err(|f| f.words(timeout))
        }
    };
    let all: Vec<String> = urls.clone();
    match tokio::time::timeout(timeout, race(urls, stagger, attempt)).await {
        Ok(Ok((url, ()))) => Ok(url),
        Ok(Err(failed)) => Err(failed),
        Err(_) => Err(all
            .into_iter()
            .map(|u| {
                (
                    u,
                    format!("no answer within {} s (lmgw.timeout_s)", timeout.as_secs()),
                )
            })
            .collect()),
    }
}

/// The offer's POST (§7.3 step 3): lmgw's answer, or why not.
pub async fn post(
    tls: &Tls,
    base: &str,
    req: &Request,
    timeout: Duration,
) -> Result<CallAnswer, Fail> {
    let (status, location, body) = exchange(tls, base, req, timeout).await?;
    calls::read_call(status, location.as_deref(), &body)
        .map_err(|error| Fail::Refused { status, error })
}

/// The hangup of a stale call (§7.8); a 404 means lmgw ended it already, as good as a 200.
pub async fn hangup(
    tls: &Tls,
    base: &str,
    call_id: &str,
    timeout: Duration,
) -> Result<HungUp, Fail> {
    let (status, _, body) = exchange(tls, base, &calls::hangup(call_id), timeout).await?;
    calls::read_hangup(status, &body).map_err(|error| Fail::Refused { status, error })
}

/// A refusal's code is a key refusal of the device (§5.5): the link acts on it.
pub fn key_refusal(status: u16, error: &ApiError) -> bool {
    status == 401
        || requests::key_refused(error).is_some_and(|k| {
            matches!(
                k,
                requests::KeyRefused::Unconfirmed
                    | requests::KeyRefused::CredentialOnDeviceListener
            )
        })
}
