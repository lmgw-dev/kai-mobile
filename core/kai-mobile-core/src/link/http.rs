//! One request to lmgw over the paired link (design §3.4's waits): `lmgw-client`'s sans-IO
//! [`Request`] sent with the one constructor's client to the URL that last connected, its head
//! within `lmgw.timeout_s`, a JSON answer's body within the same bound again. The request names the
//! pairing it went under ([`Handle::identity`]), so a refusal of an old pairing's request is not
//! acted on after a re-pair.

use std::time::Duration;

use lmgw_client::requests::{Method, Request};

use super::{failure::Failure, tls::Tls};

/// Where a request goes and with what (cheap to clone).
#[derive(Debug, Clone)]
pub struct Handle {
    pub tls: Tls,
    /// The base URL: the one that last connected, else the list's first.
    pub base: String,
}

impl Handle {
    /// The pairing the request goes under.
    pub fn identity(&self) -> [u8; 32] {
        self.tls.identity()
    }
}

/// Why a request got no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoAnswer {
    /// The connection failed (refused, a handshake, …).
    Failed(Failure),
    /// No head (or no body) within `lmgw.timeout_s`.
    Timeout,
}

impl NoAnswer {
    pub fn words(&self, timeout: Duration) -> String {
        match self {
            Self::Failed(f) => format!("lmgw is not reachable: {}", f.words()),
            Self::Timeout => format!(
                "lmgw did not answer within {} s (lmgw.timeout_s)",
                timeout.as_secs()
            ),
        }
    }
}

fn builder(h: &Handle, req: &Request) -> reqwest::RequestBuilder {
    let http = h.tls.http();
    let url = req.url(&h.base);
    let mut rb = match req.method {
        Method::Get => http.get(url),
        Method::Post => http.post(url),
    };
    for header in &req.headers {
        rb = rb.header(header.name.as_str(), header.value.as_str());
    }
    if let Some(body) = &req.body {
        rb = rb.body(body.clone());
    }
    rb
}

/// Sends `req` and reads its whole answer: the status and the body.
pub async fn call(h: &Handle, req: &Request, timeout: Duration) -> Result<(u16, String), NoAnswer> {
    let resp = open(h, req, timeout).await?;
    let status = resp.status().as_u16();
    let body = tokio::time::timeout(timeout, resp.text())
        .await
        .map_err(|_| NoAnswer::Timeout)?
        .map_err(|e| NoAnswer::Failed(Failure::of(&e)))?;
    Ok((status, body))
}

/// Sends `req` and returns its answer once the head came (for a stream: a send's, an approval's).
pub async fn open(
    h: &Handle,
    req: &Request,
    timeout: Duration,
) -> Result<reqwest::Response, NoAnswer> {
    tokio::time::timeout(timeout, builder(h, req).send())
        .await
        .map_err(|_| NoAnswer::Timeout)?
        .map_err(|e| NoAnswer::Failed(Failure::of(&e)))
}
