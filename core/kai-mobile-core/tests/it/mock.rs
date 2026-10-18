//! A scripted lmgw behind the tests' device listener ([`super::pki::Gateway`]): HTTP/1.1 with
//! keep-alive over the TLS stream, the routes the core uses answered from [`State`] (lmgw's own
//! types, serialized), each request logged with whether its connection presented a client
//! certificate. A feed request hands the test a [`Feed`] to push SSE text through; a refusal
//! queued in [`State::feed_answers`] answers the next feed request instead.

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use kai_mobile_core::lmgw::{
    requests::{CurrentThread, Folder, FolderList, Message, ThreadDetail, ThreadList, ThreadRow},
    types::{chat::Thread, chat_folders::FolderOngoing},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};

use super::pki::{Clients, Gateway, Pki};

/// One request as the mock saw it.
#[derive(Debug, Clone)]
pub struct Req {
    pub method: String,
    pub path: String,
    pub head: String,
    pub body: Vec<u8>,
    /// The connection presented a client certificate.
    pub cert: bool,
    /// Which TLS connection carried it (counted from 1).
    pub conn: usize,
}

impl Req {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case(name).then(|| v.trim())
        })
    }
}

/// What the enrolment answers.
pub enum Enrol {
    /// A status and a JSON body.
    Answer(u16, String),
    /// The connection is dropped without an answer (a lost answer).
    Drop,
}

type EnrolFn = Box<dyn FnMut(&[u8]) -> Enrol + Send>;

/// The mock's data (module doc).
#[derive(Default)]
pub struct State {
    pub folders: Vec<Folder>,
    pub threads: Vec<ThreadRow>,
    pub details: HashMap<i64, ThreadDetail>,
    /// The folder's current thread, and the thread `{new: true}` answers.
    pub current: i64,
    pub new_thread: Option<i64>,
    /// Answers for the next feed requests (status, JSON body) before a stream is held.
    pub feed_answers: VecDeque<(u16, String)>,
    pub enrol: Option<EnrolFn>,
    /// The SSE text a send answers.
    pub send_sse: String,
    /// A send answered after a wait with a refusal (status, JSON) instead.
    pub send_refusal: Option<(Duration, u16, String)>,
    /// The SSE text an approvals POST answers, or a refusal (status, JSON).
    pub approvals_answer: Option<(u16, String)>,
    /// How many of the next folder lists, and thread lists, answer 500 instead.
    pub fail_folders: usize,
    pub fail_threads: usize,
    /// What the next calls POSTs answer (status, JSON body) before the default: 201 with a call
    /// (`rtc_<n>`, an SDP answer advertising 262144 bytes).
    pub call_answers: VecDeque<(u16, String)>,
    /// The calls answered so far.
    pub calls: usize,
    /// What a hangup answers; 0 = 200.
    pub hangup_status: u16,
    pub log: Vec<Req>,
}

/// A held feed: push SSE text through it; dropping it ends the stream.
pub struct Feed {
    pub req: Req,
    tx: mpsc::UnboundedSender<String>,
}

impl Feed {
    pub fn send(&self, sse: &str) {
        let _ = self.tx.send(sse.to_owned());
    }
}

pub struct Mock {
    pub url: String,
    pub state: Arc<Mutex<State>>,
    feeds: tokio::sync::Mutex<mpsc::UnboundedReceiver<Feed>>,
    connections: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Mock {
    /// A mock of `pki`'s gateway, client certificates optional (as while a token is pending) or
    /// required.
    pub async fn start(pki: &Pki, clients: Clients) -> Self {
        let gateway = Gateway::bind_as(pki, clients).await;
        let url = gateway.url.clone();
        let state = Arc::new(Mutex::new(State::default()));
        let (feeds_tx, feeds) = mpsc::unbounded_channel();
        let connections = Arc::new(AtomicUsize::new(0));
        let task = {
            let state = state.clone();
            let connections = connections.clone();
            tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = gateway.accept().await else {
                        continue;
                    };
                    let conn = connections.fetch_add(1, Ordering::SeqCst) + 1;
                    let cert = stream
                        .get_ref()
                        .1
                        .peer_certificates()
                        .is_some_and(|c| !c.is_empty());
                    tokio::spawn(serve(stream, conn, cert, state.clone(), feeds_tx.clone()));
                }
            })
        };
        Self {
            url,
            state,
            feeds: tokio::sync::Mutex::new(feeds),
            connections,
            task,
        }
    }

    /// The next feed the core holds.
    pub async fn feed(&self) -> Feed {
        tokio::time::timeout(Duration::from_secs(15), self.feeds.lock().await.recv())
            .await
            .expect("a feed request")
            .expect("the mock runs")
    }

    /// Whether a feed request comes within `wait`.
    pub async fn feed_within(&self, wait: Duration) -> Option<Feed> {
        tokio::time::timeout(wait, self.feeds.lock().await.recv())
            .await
            .ok()
            .flatten()
    }

    /// How many TLS connections were made.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    pub fn requests(&self, path_prefix: &str) -> Vec<Req> {
        self.state
            .lock()
            .unwrap()
            .log
            .iter()
            .filter(|r| r.path.starts_with(path_prefix))
            .cloned()
            .collect()
    }

    /// The Kai folder (ongoing, with a model) and its current thread `thread` with `messages`.
    pub fn with_conversation(&self, folder_id: i64, thread: i64, messages: Vec<Message>) {
        let mut s = self.state.lock().unwrap();
        s.folders = vec![ready_folder(folder_id, "Kai", thread)];
        s.threads = vec![ThreadRow {
            id: thread,
            title: "Kai".into(),
            folder_id: Some(folder_id),
            ..Default::default()
        }];
        s.details.insert(thread, detail(thread, "Kai", messages));
        s.current = thread;
    }
}

/// An ongoing folder with a model.
pub fn ready_folder(id: i64, name: &str, thread: i64) -> Folder {
    let mut f = Folder {
        id,
        name: name.into(),
        ongoing: Some(FolderOngoing {
            idle_minutes: 120,
            current_thread_id: Some(thread),
        }),
        ..Default::default()
    };
    f.defaults.model_alias = Some("gemma".into());
    f
}

pub fn detail(thread: i64, title: &str, messages: Vec<Message>) -> ThreadDetail {
    ThreadDetail {
        thread: Thread {
            row: ThreadRow {
                id: thread,
                title: title.into(),
                ..Default::default()
            },
            ..Default::default()
        },
        messages,
        ..Default::default()
    }
}

pub fn message(id: i64, thread: i64, role: &str, content: &str) -> Message {
    Message {
        id,
        thread_id: thread,
        role: role.into(),
        content: content.into(),
        created_at: "2026-10-15 10:00:00".into(),
        ..Default::default()
    }
}

fn answer(status: u16, body: &str, close: bool) -> String {
    format!(
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\n{}\r\n{body}",
        body.len(),
        if close { "connection: close\r\n" } else { "" }
    )
}

/// A 500's body.
const BROKEN: &str = r#"{"code":"internal","message":"the database is locked"}"#;

const SSE_HEAD: &str = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n";

/// One request's head and body (by its `Content-Length`); `None` when the connection ended.
async fn read_request<S: tokio::io::AsyncRead + Unpin>(
    s: &mut S,
    buf: &mut Vec<u8>,
) -> Option<(String, Vec<u8>)> {
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break end;
        }
        let n = s.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8(buf[..end].to_vec()).ok()?;
    let len: usize = head
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse().ok())?
        })
        .unwrap_or(0);
    let mut rest = buf.split_off(end + 4);
    while rest.len() < len {
        let n = s.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        rest.extend_from_slice(&chunk[..n]);
    }
    *buf = rest.split_off(len);
    Some((head, rest))
}

/// What a request gets.
enum Action {
    Reply(String),
    /// An answer after a wait.
    Later(Duration, String),
    /// An event stream's whole text, then the close.
    Stream(String),
    /// A held feed.
    Hold,
    /// The connection dropped without an answer.
    Drop,
}

async fn serve<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send>(
    mut s: S,
    conn: usize,
    cert: bool,
    state: Arc<Mutex<State>>,
    feeds: mpsc::UnboundedSender<Feed>,
) {
    let mut buf = Vec::new();
    while let Some((head, body)) = read_request(&mut s, &mut buf).await {
        let line = head.lines().next().unwrap_or_default().to_owned();
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_owned();
        let path = parts.next().unwrap_or_default().to_owned();
        let req = Req {
            method: method.clone(),
            path: path.clone(),
            head: head.clone(),
            body: body.clone(),
            cert,
            conn,
        };
        state.lock().unwrap().log.push(req.clone());
        let route = path.split('?').next().unwrap_or_default().to_owned();
        let action: Action = {
            let mut st = state.lock().unwrap();
            match (method.as_str(), route.as_str()) {
                ("GET", "/chat/api/feed") => match st.feed_answers.pop_front() {
                    Some((status, body)) => Action::Reply(answer(status, &body, false)),
                    None => Action::Hold,
                },
                ("POST", "/devices/enrol") => match st.enrol.as_mut().map(|f| f(&body)) {
                    Some(Enrol::Answer(status, body)) => Action::Reply(answer(status, &body, true)),
                    Some(Enrol::Drop) => Action::Drop,
                    None => Action::Reply(answer(
                        401,
                        r#"{"code":"pairing_token_unknown","message":"unknown"}"#,
                        true,
                    )),
                },
                ("GET", "/chat/api/folders") if st.fail_folders > 0 => {
                    st.fail_folders -= 1;
                    Action::Reply(answer(500, BROKEN, false))
                }
                ("GET", "/chat/api/threads") if st.fail_threads > 0 => {
                    st.fail_threads -= 1;
                    Action::Reply(answer(500, BROKEN, false))
                }
                ("GET", "/chat/api/folders") => Action::Reply(answer(
                    200,
                    &serde_json::to_string(&FolderList {
                        folders: st.folders.clone(),
                    })
                    .unwrap(),
                    false,
                )),
                ("POST", "/chat/api/folders") => {
                    let id = st.folders.iter().map(|f| f.id).max().unwrap_or(0) + 1;
                    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                    let f = Folder {
                        id,
                        name: v["name"].as_str().unwrap_or_default().into(),
                        ..Default::default()
                    };
                    st.folders.push(f.clone());
                    Action::Reply(answer(201, &serde_json::to_string(&f).unwrap(), false))
                }
                ("GET", "/chat/api/threads") => Action::Reply(answer(
                    200,
                    &serde_json::to_string(&ThreadList {
                        threads: st.threads.clone(),
                        ..Default::default()
                    })
                    .unwrap(),
                    false,
                )),
                ("POST", r) if r.ends_with("/current") => {
                    let new = String::from_utf8_lossy(&body).contains("true");
                    if new && let Some(t) = st.new_thread {
                        st.current = t;
                    }
                    let id = st.current;
                    let row = st
                        .details
                        .get(&id)
                        .map(|d| d.thread.clone())
                        .unwrap_or_default();
                    Action::Reply(answer(
                        200,
                        &serde_json::to_string(&CurrentThread {
                            thread: row,
                            rolled_over: new,
                            ..Default::default()
                        })
                        .unwrap(),
                        false,
                    ))
                }
                // The warm-up: the device listener's 404 for a path without a GET route.
                ("GET", "/v1/realtime/calls") => Action::Reply(answer(404, "", false)),
                ("POST", "/v1/realtime/calls") => match st.call_answers.pop_front() {
                    Some((status, body)) => Action::Reply(answer(status, &body, false)),
                    None => {
                        st.calls += 1;
                        let sdp = "v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\ns=-\r\na=ice-lite\r\na=max-message-size:262144\r\n";
                        Action::Reply(format!(
                            "HTTP/1.1 201 Created\r\ncontent-type: application/sdp\r\nlocation: /v1/realtime/calls/rtc_{}\r\ncontent-length: {}\r\n\r\n{sdp}",
                            st.calls,
                            sdp.len()
                        ))
                    }
                },
                ("POST", r) if r.starts_with("/v1/realtime/calls/") && r.ends_with("/hangup") => {
                    match st.hangup_status {
                        0 | 200 => Action::Reply(answer(200, "", false)),
                        status => Action::Reply(answer(
                            status,
                            r#"{"error":{"type":"invalid_request_error","code":"call_not_found","message":"no such call"}}"#,
                            false,
                        )),
                    }
                }
                ("POST", r) if r.ends_with("/send") => match st.send_refusal.clone() {
                    Some((wait, status, body)) => Action::Later(wait, answer(status, &body, true)),
                    None => Action::Stream(st.send_sse.clone()),
                },
                ("POST", r) if r.ends_with("/approvals") => match st.approvals_answer.clone() {
                    Some((status, body)) if status != 200 => {
                        Action::Reply(answer(status, &body, false))
                    }
                    other => Action::Stream(other.map(|(_, b)| b).unwrap_or_else(|| {
                        "event: done\ndata: {\"aborted\":false}\n\n".to_owned()
                    })),
                },
                ("GET", r) if r.starts_with("/chat/api/threads/") => {
                    let id: i64 = r
                        .trim_start_matches("/chat/api/threads/")
                        .parse()
                        .unwrap_or(0);
                    match st.details.get(&id) {
                        Some(d) => {
                            Action::Reply(answer(200, &serde_json::to_string(d).unwrap(), false))
                        }
                        None => Action::Reply(answer(
                            404,
                            r#"{"code":"not_found","message":"no thread"}"#,
                            false,
                        )),
                    }
                }
                _ => Action::Reply(answer(
                    404,
                    r#"{"code":"not_found","message":"no route"}"#,
                    false,
                )),
            }
        };
        match action {
            Action::Drop => return,
            Action::Later(wait, r) => {
                tokio::time::sleep(wait).await;
                let _ = s.write_all(r.as_bytes()).await;
                let _ = s.shutdown().await;
                return;
            }
            Action::Stream(sse) => {
                let _ = s.write_all(SSE_HEAD.as_bytes()).await;
                let _ = s.write_all(sse.as_bytes()).await;
                let _ = s.shutdown().await;
                return;
            }
            Action::Reply(r) => {
                if s.write_all(r.as_bytes()).await.is_err() {
                    return;
                }
                if r.contains("connection: close") {
                    let _ = s.shutdown().await;
                    return;
                }
            }
            Action::Hold => {
                // A held feed.
                if s.write_all(SSE_HEAD.as_bytes()).await.is_err() {
                    return;
                }
                let (tx, mut rx) = mpsc::unbounded_channel::<String>();
                if feeds.send(Feed { req, tx }).is_err() {
                    return;
                }
                while let Some(text) = rx.recv().await {
                    if s.write_all(text.as_bytes()).await.is_err() {
                        return;
                    }
                    let _ = s.flush().await;
                }
                let _ = s.shutdown().await;
                return;
            }
        }
    }
}
