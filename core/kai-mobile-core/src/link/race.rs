//! The URL race (design M5, §5.2; device link §6.3), as Kai desktop runs it: each URL in
//! `Urls::order`'s order (the one that last connected first, then the link's), the next one started
//! `lmgw.url_stagger_ms` after the previous one unless that one's connection is up, at once after a
//! failure; the first answer wins and the rest are dropped. "Up" is the TCP connection and the TLS
//! handshake that verified lmgw ([`Answered`]): each attempt's HTTP client is its own
//! (`super::tls::Tls::racing`), with a layer on reqwest's connector that says when its connection
//! is made, so a gateway that is reached but slow to answer is not raced by the next URL. The
//! caller bounds the whole race by `lmgw.timeout_s`.

use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use futures_util::{StreamExt, stream::FuturesUnordered};
use tokio::sync::watch;

/// Says when an attempt's connection is up (TCP, and TLS with lmgw verified). Cheap to clone.
#[derive(Clone)]
pub struct Answered(Arc<watch::Sender<bool>>);

impl Answered {
    /// A fresh one, and what the race watches.
    pub fn new() -> (Self, watch::Receiver<bool>) {
        let (tx, rx) = watch::channel(false);
        (Self(Arc::new(tx)), rx)
    }

    /// The connection is up.
    pub fn up(&self) {
        self.0.send_replace(true);
    }
}

/// The layer on reqwest's connector that says [`Answered`] once a connection is made: reqwest's
/// connector returns it after TCP and the TLS handshake.
#[derive(Clone)]
pub(crate) struct OnConnect(pub(crate) Answered);

impl<S> tower_layer::Layer<S> for OnConnect {
    type Service = OnConnectService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        OnConnectService {
            inner,
            answered: self.0.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct OnConnectService<S> {
    inner: S,
    answered: Answered,
}

impl<S, R> tower_service::Service<R> for OnConnectService<S>
where
    S: tower_service::Service<R>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: R) -> Self::Future {
        let connecting = self.inner.call(req);
        let answered = self.answered.clone();
        Box::pin(async move {
            let made = connecting.await;
            if made.is_ok() {
                answered.up();
            }
            made
        })
    }
}

/// The rule (module doc): `attempt` for each of `urls` in order, with an [`Answered`] it hands to
/// its connection; the next started `stagger` after the previous one unless that one's connection
/// is up or it has ended, at once after a failure; the first that succeeds wins and the rest are
/// dropped. All failed: each URL's error, in the list's order.
pub async fn race<T, E, F, Fut>(
    urls: Vec<String>,
    stagger: Duration,
    mut attempt: F,
) -> Result<(String, T), Vec<(String, E)>>
where
    F: FnMut(&str, Answered) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut waiting = urls.into_iter().enumerate();
    let mut running = FuturesUnordered::new();
    let mut failed: Vec<(usize, String, E)> = Vec::new();
    // The latest attempt's connection, and when the next starts unless it is up first.
    let mut latest: Option<watch::Receiver<bool>> = None;
    let mut start = |running: &mut FuturesUnordered<_>, latest: &mut Option<_>| match waiting.next()
    {
        Some((i, url)) => {
            let (answered, up) = Answered::new();
            let fut = attempt(&url, answered);
            running.push(async move { (i, url, fut.await) });
            *latest = Some(up);
            Some(tokio::time::Instant::now() + stagger)
        }
        None => None,
    };
    let mut next_at = start(&mut running, &mut latest);
    if running.is_empty() {
        return Err(Vec::new());
    }
    loop {
        let timed = next_at.is_some();
        tokio::select! {
            Some((i, url, r)) = running.next() => match r {
                Ok(t) => return Ok((url, t)),
                Err(e) => {
                    failed.push((i, url, e));
                    next_at = start(&mut running, &mut latest);
                    if next_at.is_none() && running.is_empty() {
                        failed.sort_by_key(|(i, ..)| *i);
                        return Err(failed.into_iter().map(|(_, u, e)| (u, e)).collect());
                    }
                }
            },
            () = tokio::time::sleep_until(next_at.unwrap_or_else(tokio::time::Instant::now)), if timed => {
                next_at = start(&mut running, &mut latest);
            }
            // The latest attempt reached lmgw: the next URL waits for it to fail. An attempt that
            // ended says so through `running`.
            () = async {
                let reached = match latest.as_mut() {
                    Some(up) => up.wait_for(|up| *up).await.is_ok(),
                    None => false,
                };
                if !reached {
                    std::future::pending::<()>().await;
                }
            }, if timed => {
                next_at = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[tokio::test]
    async fn the_next_url_starts_after_the_stagger_and_the_first_answer_wins() {
        let t0 = Instant::now();
        let r: Result<(String, u32), Vec<(String, String)>> = race(
            vec!["a".into(), "b".into()],
            Duration::from_millis(50),
            |url, _answered| {
                let url = url.to_owned();
                async move {
                    if url == "a" {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        Ok(1)
                    } else {
                        Ok(2)
                    }
                }
            },
        )
        .await;
        assert_eq!(r.unwrap(), ("b".into(), 2));
        let took = t0.elapsed();
        assert!(
            took >= Duration::from_millis(50) && took < Duration::from_secs(2),
            "{took:?}"
        );
    }

    #[tokio::test]
    async fn a_connection_that_is_up_holds_the_next_url_back() {
        let started = Arc::new(std::sync::Mutex::new(Vec::new()));
        let s = started.clone();
        let r: Result<(String, u32), Vec<(String, String)>> = race(
            vec!["a".into(), "b".into()],
            Duration::from_millis(20),
            move |url, answered| {
                s.lock().unwrap().push(url.to_owned());
                async move {
                    answered.up();
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok(1)
                }
            },
        )
        .await;
        assert_eq!(r.unwrap().0, "a");
        assert_eq!(*started.lock().unwrap(), ["a"], "b never started");
    }

    #[tokio::test]
    async fn a_failure_starts_the_next_at_once_and_all_failed_says_each() {
        let t0 = Instant::now();
        let r: Result<(String, u32), Vec<(String, String)>> = race(
            vec!["a".into(), "b".into()],
            Duration::from_secs(5),
            |url, _| {
                let url = url.to_owned();
                async move { Err(format!("{url} refused")) }
            },
        )
        .await;
        assert!(t0.elapsed() < Duration::from_secs(1));
        assert_eq!(
            r.unwrap_err(),
            [
                ("a".to_owned(), "a refused".to_owned()),
                ("b".to_owned(), "b refused".to_owned())
            ]
        );
    }
}
