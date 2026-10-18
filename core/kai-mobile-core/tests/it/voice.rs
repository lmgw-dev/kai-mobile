//! The call's signalling and its task (design §15.3, "the call's signalling"): the POST by the Kai
//! folder with `takeover=ask` over the device link, on the warm connection; the `session` part with
//! and without an override; `new`; the folder's refusals (404 looks the folder up again); a refusal
//! at the POST; Reconnect's hangup of a stale id (a 404 counts as done); the microphone `Off` when
//! the feed shows another holder; a call through its events to the End; a key refusal reaching the
//! link; no reply notification while a call runs; the assistant overlay's dismissal.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use kai_mobile_core::{
    CallStart, CoreError, EventSink, MicState, PlayoutStats, TransportState, Voice,
    store::{self, CallsFile},
    views::{ConnectionState, ConversationStatus, Update},
    voice::{CallEnd, CallPhase, EndReason},
};
use serde_json::json;
use tokio::sync::mpsc;

use super::{
    mock::{Mock, ready_folder},
    pki::{Clients, PKI},
    rig::{Rig, caught_up, fast_settings, hello, reply, requested, stored},
};

/// How the feed names the rig's phone (`hello`'s principal).
const PHONE: &str = "device 'phone'";

/// An `approval.decided` record's data: decided `approve` by `by`.
fn decided_by(id: &str, approve: bool, by: &str) -> serde_json::Value {
    json!({"thread_id": 7, "message_id": 30, "approval_request_id": id, "approve": approve,
        "by": by})
}

/// The notices said so far.
fn notices(rig: &Rig) -> Vec<String> {
    rig.seen
        .iter()
        .filter_map(|u| match u {
            Update::Notice { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// The call ids `calls.json` keeps (§7.8): those lmgw may still hold.
fn owed(rig: &Rig) -> Vec<String> {
    store::read::<CallsFile>(&rig.dir.path().join(store::CALLS_FILE))
        .unwrap()
        .map(|f| f.owed)
        .unwrap_or_default()
}

/// The paths of the POSTs to lmgw's calls (the offers and the hangups), in order.
fn call_posts(mock: &Mock) -> Vec<String> {
    mock.requests("/v1/realtime/calls")
        .into_iter()
        .filter(|r| r.method == "POST")
        .map(|r| r.path)
        .collect()
}

/// A libwebrtc offer (the POST carries it as it is; the mock does not read it).
const OFFER: &str = "v=0\r\no=- 46117 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=group:BUNDLE 0 1\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=rtpmap:111 opus/48000/2\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n";

/// Kotlin's side as the tests fake it: every call as a line, in order.
struct Sink {
    tx: mpsc::UnboundedSender<String>,
    ends: Arc<Mutex<Vec<CallEnd>>>,
}

impl EventSink for Sink {
    fn send(&self, text: String) {
        let _ = self.tx.send(format!("send {text}"));
    }
    fn set_mic(&self, mic: MicState) {
        let _ = self.tx.send(format!("mic {mic:?}"));
    }
    fn set_playback(&self, audible: bool) {
        let _ = self.tx.send(format!("playback {audible}"));
    }
    fn read_stats(&self, request: u64) {
        let _ = self.tx.send(format!("stats {request}"));
    }
    fn close_channel(&self) {
        let _ = self.tx.send("close_channel".into());
    }
    fn close_peer(&self) {
        let _ = self.tx.send("close_peer".into());
    }
    fn cue(&self) {
        let _ = self.tx.send("cue".into());
    }
    fn asked(&self) {
        let _ = self.tx.send("asked".into());
    }
    fn ended(&self, end: CallEnd) {
        self.ends.lock().unwrap().push(end.clone());
        let _ = self.tx.send(format!("ended {:?}", end.reason));
    }
}

struct Kotlin {
    rx: mpsc::UnboundedReceiver<String>,
    ends: Arc<Mutex<Vec<CallEnd>>>,
}

impl Kotlin {
    fn new() -> (Box<dyn EventSink>, Self) {
        let (tx, rx) = mpsc::unbounded_channel();
        let ends = Arc::new(Mutex::new(Vec::new()));
        (
            Box::new(Sink {
                tx,
                ends: ends.clone(),
            }),
            Self { rx, ends },
        )
    }

    /// The next line the sink got, within 10 s.
    async fn next(&mut self) -> String {
        tokio::time::timeout(Duration::from_secs(10), self.rx.recv())
            .await
            .expect("a sink call")
            .expect("the sink lives")
    }

    /// The lines up to one starting with `prefix` (that one included).
    async fn until(&mut self, prefix: &str) -> Vec<String> {
        let mut seen = Vec::new();
        loop {
            let l = self.next().await;
            let done = l.starts_with(prefix);
            seen.push(l);
            if done {
                return seen;
            }
        }
    }
}

/// A paired core whose feed is up and whose conversation is ready (folder 5, thread 7); `voice`
/// the holders the feed's `hello` names.
async fn connected(mock: &Mock, voice: serde_json::Value) -> (Rig, super::mock::Feed) {
    mock.with_conversation(5, 7, vec![]);
    let mut rig = Rig::paired(&PKI, &[&mock.url]).await;
    let feed = mock.feed().await;
    let h = json!({"epoch": "e1", "cursor": "e1:4:aa", "keepalive_s": 120, "retention_days": 7,
        "principal": {"kind": "device", "name": "phone"},
        "hold": {"active": false, "fallback_alias": null}, "voice": voice, "turns": [],
        "open_approvals": []});
    feed.send(&format!("event: hello\ndata: {h}\n\n"));
    feed.send(&caught_up("e1:4:aa"));
    rig.next(
        "ready",
        |u| matches!(u, Update::Conversation { view } if view.status == ConversationStatus::Ready),
    )
    .await;
    (rig, feed)
}

fn posts(mock: &Mock) -> Vec<super::mock::Req> {
    mock.requests("/v1/realtime/calls")
        .into_iter()
        .filter(|r| r.method == "POST" && !r.path.contains("/hangup"))
        .collect()
}

/// The POST by the folder with `takeover=ask`, the raw offer without an override, on the warm
/// connection with the device's certificate; lmgw's answer handed back.
#[tokio::test]
async fn the_offer_goes_by_the_folder_on_the_warm_connection() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (rig, _feed) = connected(&mock, json!([])).await;
    let voice: Arc<Voice> = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    assert_eq!(k.next().await, "mic Live");
    let answer = voice.offer(n, OFFER.into()).await.unwrap();
    assert!(answer.contains("a=max-message-size:262144"), "{answer}");
    let post = &posts(&mock)[0];
    assert_eq!(post.path, "/v1/realtime/calls?chat_folder=5&takeover=ask");
    assert_eq!(post.header("content-type"), Some("application/sdp"));
    assert_eq!(String::from_utf8_lossy(&post.body), OFFER);
    assert!(post.cert, "the device's certificate");
    let warm = mock
        .requests("/v1/realtime/calls")
        .into_iter()
        .find(|r| r.method == "GET")
        .expect("the warm-up");
    assert_eq!(warm.conn, post.conn, "the POST took the warm connection");
    // A second offer for the same call is refused.
    assert!(voice.offer(n, OFFER.into()).await.is_err());
    // As is an offer that names another call.
    assert!(voice.offer(n + 1, OFFER.into()).await.is_err());
}

/// The owner's override travels as the `session` part (multipart), and `new=true` after New
/// conversation.
#[tokio::test]
async fn the_session_part_carries_the_override_and_new_rolls_the_folder_over() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock.with_conversation(5, 7, vec![]);
    let mut settings = fast_settings();
    settings.voice.mode = kai_mobile_core::VoiceMode::PushToTalk;
    let mut rig = Rig::paired_with(&PKI, &[&mock.url], settings).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.next(
        "ready",
        |u| matches!(u, Update::Conversation { view } if view.status == ConversationStatus::Ready),
    )
    .await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice
        .start(
            CallStart {
                new: true,
                take_over: false,
            },
            sink,
        )
        .await
        .unwrap();
    assert_eq!(k.next().await, "mic Silent", "push-to-talk between turns");
    voice.offer(n, OFFER.into()).await.unwrap();
    let post = &posts(&mock)[0];
    assert_eq!(
        post.path,
        "/v1/realtime/calls?chat_folder=5&new=true&takeover=ask"
    );
    let ct = post.header("content-type").unwrap();
    assert!(ct.starts_with("multipart/form-data; boundary="), "{ct}");
    let body = String::from_utf8_lossy(&post.body);
    assert!(body.contains("name=\"sdp\""), "{body}");
    assert!(body.contains(OFFER));
    assert!(body.contains("name=\"session\""));
    assert!(
        body.contains(r#"{"lmgw":{"turn_mode":"push_to_talk"},"type":"realtime"}"#),
        "{body}"
    );
}

/// `voice.echo` `half_duplex` travels as the `session` part with `voice.echo_tail_ms` (§7.4, §8.2),
/// read at the call's start.
#[tokio::test]
async fn half_duplex_travels_as_the_session_part_with_its_tail() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    mock.with_conversation(5, 7, vec![]);
    let mut settings = fast_settings();
    settings.voice.echo = kai_mobile_core::EchoMode::HalfDuplex;
    settings.voice.echo_tail_ms = Some(250);
    let mut rig = Rig::paired_with(&PKI, &[&mock.url], settings).await;
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    rig.next(
        "ready",
        |u| matches!(u, Update::Conversation { view } if view.status == ConversationStatus::Ready),
    )
    .await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    assert_eq!(
        k.next().await,
        "mic Live",
        "half duplex keeps the microphone open"
    );
    voice.offer(n, OFFER.into()).await.unwrap();
    let post = &posts(&mock)[0];
    assert_eq!(post.path, "/v1/realtime/calls?chat_folder=5&takeover=ask");
    let body = String::from_utf8_lossy(&post.body);
    assert!(
        body.contains(r#"{"lmgw":{"echo_tail_ms":250,"half_duplex":true},"type":"realtime"}"#),
        "{body}"
    );
}

/// 404 `chat_folder_not_found` looks the folder up again (§6.1's rule) and POSTs once more.
#[tokio::test]
async fn a_folder_lmgw_does_not_know_is_looked_up_again() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (rig, _feed) = connected(&mock, json!([])).await;
    {
        let mut st = mock.state.lock().unwrap();
        st.call_answers.push_back((
            404,
            json!({"error": {"type": "invalid_request_error", "code": "chat_folder_not_found",
                "message": "chat folder 5 not found"}})
            .to_string(),
        ));
        // The folder was made again under another id.
        st.folders = vec![ready_folder(6, "Kai", 7)];
    }
    let voice = rig.core.voice();
    let (sink, _k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    let paths: Vec<String> = posts(&mock).into_iter().map(|r| r.path).collect();
    assert_eq!(
        paths,
        [
            "/v1/realtime/calls?chat_folder=5&takeover=ask",
            "/v1/realtime/calls?chat_folder=6&takeover=ask"
        ]
    );
}

/// A refusal at the POST is answered with its code, and the call has ended.
#[tokio::test]
async fn a_refused_post_ends_the_call_with_its_words() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (rig, _feed) = connected(&mock, json!([])).await;
    mock.state.lock().unwrap().call_answers.push_back((
        503,
        json!({"error": {"type": "server_error", "code": "webrtc_unavailable",
            "message": "the UDP listener is off (realtime.webrtc_enabled)"}})
        .to_string(),
    ));
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    let err = voice.offer(n, OFFER.into()).await.unwrap_err();
    let CoreError::Refused { code, reason, .. } = err else {
        panic!("{err:?}")
    };
    assert_eq!(code, "webrtc_unavailable");
    assert!(reason.contains("realtime.webrtc_enabled"));
    k.until("ended Refused").await;
    let end = k.ends.lock().unwrap()[0].clone();
    assert!(end.words.unwrap().contains("(webrtc_unavailable)"));
    assert_eq!(k.until("close_peer").await.last().unwrap(), "close_peer");
}

/// A call lost without lmgw's word leaves its id stale: the next start hangs it up first, and a
/// 404 (lmgw ended it already) counts as done.
#[tokio::test]
async fn reconnect_hangs_up_the_stale_call_first() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (rig, _feed) = connected(&mock, json!([])).await;
    mock.state.lock().unwrap().hangup_status = 404;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Failed, "failed".into());
    k.until("ended ConnectionLost").await;
    k.until("close_peer").await;
    let (sink, _k2) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    let order: Vec<String> = mock
        .requests("/v1/realtime/calls")
        .into_iter()
        .filter(|r| r.method == "POST")
        .map(|r| r.path)
        .collect();
    assert_eq!(
        order,
        [
            "/v1/realtime/calls?chat_folder=5&takeover=ask",
            "/v1/realtime/calls/rtc_1/hangup",
            "/v1/realtime/calls?chat_folder=5&takeover=ask"
        ]
    );
    // Ended before its channel opened: given up after its POST, so hung up at the next start.
    voice.end();
    let (sink, _k3) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    let hangups = mock
        .requests("/v1/realtime/calls/")
        .into_iter()
        .filter(|r| r.path.ends_with("/hangup"))
        .count();
    assert_eq!(
        hangups, 2,
        "rtc_2 ended before its channel opened: hung up too"
    );
}

/// An ended call's PeerConnection may still be closing when the next call starts (Reconnect, Take
/// over): its `closed` (libwebrtc reports it as `close()` begins), its channel's close and its
/// stats name the old call and reach no other.
#[tokio::test]
async fn an_old_call_s_events_never_reach_the_next_call() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (rig, _feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, mut k1) = Kotlin::new();
    let old = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(old, OFFER.into()).await.unwrap();
    voice.transport(old, TransportState::Connected, "connected".into());
    voice.channel_open(old);
    // End before the bind: the channel's close alone; the PeerConnection waits for it.
    voice.end();
    k1.until("close_channel").await;
    let (sink, mut k2) = Kotlin::new();
    let new = voice.start(CallStart::default(), sink).await.unwrap();
    assert_ne!(new, old);
    assert_eq!(k1.until("close_peer").await.last().unwrap(), "close_peer");
    voice.transport(old, TransportState::Closed, "closed".into());
    voice.channel_closed(old, "closed".into());
    voice.stats(old, 1, PlayoutStats::default());
    // The new call is not over: its offer goes, and its own events still count.
    voice.offer(new, OFFER.into()).await.unwrap();
    voice.transport(new, TransportState::Connected, "connected".into());
    let lines = k2.until("cue").await;
    assert!(
        !lines.iter().any(|l| l.starts_with("ended")),
        "the old call's closed ended the new one: {lines:?}"
    );
    assert!(k2.ends.lock().unwrap().is_empty());
}

/// Every call that ended without lmgw's word stays owed until its hangup answers: an older one whose
/// hangup was refused is hung up again beside a newer one, oldest first.
#[tokio::test]
async fn every_owed_call_is_hung_up_before_the_next_bind() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (rig, _feed) = connected(&mock, json!([])).await;
    mock.state.lock().unwrap().hangup_status = 503;
    let voice = rig.core.voice();
    for _ in 0..2 {
        let (sink, mut k) = Kotlin::new();
        let n = voice.start(CallStart::default(), sink).await.unwrap();
        voice.offer(n, OFFER.into()).await.unwrap();
        voice.transport(n, TransportState::Failed, "failed".into());
        k.until("close_peer").await;
    }
    assert_eq!(owed(&rig), ["rtc_1", "rtc_2"]);
    mock.state.lock().unwrap().hangup_status = 0;
    let (sink, _k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    assert_eq!(
        call_posts(&mock),
        [
            "/v1/realtime/calls?chat_folder=5&takeover=ask",
            "/v1/realtime/calls/rtc_1/hangup",
            "/v1/realtime/calls?chat_folder=5&takeover=ask",
            "/v1/realtime/calls/rtc_1/hangup",
            "/v1/realtime/calls/rtc_2/hangup",
            "/v1/realtime/calls?chat_folder=5&takeover=ask"
        ]
    );
    assert_eq!(owed(&rig), ["rtc_3"], "the live call's id alone");
}

/// The live call's id is kept from lmgw's answer on: a process that dies mid-call hangs it up
/// before its first POST; a clean end clears it.
#[tokio::test]
async fn a_call_id_outlives_the_process_and_is_hung_up_first() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (rig, _feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, _k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    assert_eq!(owed(&rig), ["rtc_1"]);
    // The process dies mid-call.
    drop(voice);
    let mut rig = rig.restart();
    let feed = mock.feed().await;
    feed.send(&hello("e1:4:aa", 120, "[]"));
    feed.send(&caught_up("e1:4:aa"));
    rig.next(
        "ready",
        |u| matches!(u, Update::Conversation { view } if view.status == ConversationStatus::Ready),
    )
    .await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    assert_eq!(
        call_posts(&mock),
        [
            "/v1/realtime/calls?chat_folder=5&takeover=ask",
            "/v1/realtime/calls/rtc_1/hangup",
            "/v1/realtime/calls?chat_folder=5&takeover=ask"
        ]
    );
    assert_eq!(owed(&rig), ["rtc_2"]);
    // lmgw's word, then the channel's close: a clean end, nothing owed.
    voice.transport(n, TransportState::Connected, "connected".into());
    voice.channel_open(n);
    voice.channel_message(
        n,
        json!({"type": "lmgw.session.closed", "kind": "closed", "by": "the dashboard",
            "message": "the call was hung up by the dashboard"})
        .to_string(),
    );
    voice.channel_closed(n, "closed".into());
    k.until("close_peer").await;
    // A round trip through the voice task, which settles the id after the close.
    assert!(voice.offer(n, OFFER.into()).await.is_err());
    assert_eq!(owed(&rig), Vec::<String>::new());
}

/// Voice answers at once with the state's words when it cannot start.
#[tokio::test]
async fn voice_without_a_pairing_answers_at_once() {
    let rig = Rig::unpaired().await;
    let (sink, _k) = Kotlin::new();
    let err = rig
        .core
        .voice()
        .start(CallStart::default(), sink)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CoreError::NotConnected { reason } if reason.contains("Not paired")),
        "{err:?}"
    );
}

/// The feed shows another holder: the microphone is `Off` before the answer, and the prompt's
/// Take over goes alone.
#[tokio::test]
async fn another_holder_keeps_the_microphone_off_until_the_bind() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, _feed) =
        connected(&mock, json!([{"thread_id": 7, "by": "device 'desktop'"}])).await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    assert_eq!(k.next().await, "mic Off");
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    voice.channel_open(n);
    voice.channel_message(
        n,
        json!({"type": "lmgw.session.bind_pending", "held_by": "device 'desktop'"}).to_string(),
    );
    let u = rig
        .next(
            "pending",
            |u| matches!(u, Update::Voice { view } if view.phase == CallPhase::Pending),
        )
        .await;
    let Update::Voice { view } = u else {
        unreachable!()
    };
    assert_eq!(view.held_by.as_deref(), Some("device 'desktop'"));
    voice.take_over();
    assert_eq!(
        k.next().await,
        r#"send {"type":"lmgw.session.take_over"}"#,
        "no cue came before it"
    );
}

/// A call through its events: the cue, the bind, a caption, the End as the stop and the channel's
/// close, then the peer.
#[tokio::test]
async fn a_call_runs_from_the_cue_to_the_end() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, _feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    k.until("cue").await;
    voice.channel_open(n);
    voice.channel_message(
        n,
        json!({"type": "session.created", "session": {"type": "realtime",
            "lmgw": {"turn_mode": "automatic", "resolved": {"chat_thread": {"id": 7, "title": "Kai"}}}}})
        .to_string(),
    );
    rig.next(
        "live",
        |u| matches!(u, Update::Voice { view } if view.phase == CallPhase::Live),
    )
    .await;
    voice.channel_message(
        n,
        json!({"type": "conversation.item.input_audio_transcription.completed",
            "transcript": "What time is it?"})
        .to_string(),
    );
    rig.next(
        "caption",
        |u| matches!(u, Update::Caption { line } if line.text == "What time is it?"),
    )
    .await;
    voice.channel_message(
        n,
        json!({"type": "response.created", "response": {"id": "r1"}}).to_string(),
    );
    voice.channel_message(
        n,
        json!({"type": "output_audio_buffer.started", "response_id": "r1"}).to_string(),
    );
    voice.end();
    let lines = k.until("stats").await;
    assert!(lines.contains(&"playback false".to_owned()), "{lines:?}");
    let request: u64 = lines.last().unwrap()[6..].parse().unwrap();
    voice.stats(
        n,
        request,
        PlayoutStats {
            estimated_playout_timestamp: Some(1234.5),
            ..Default::default()
        },
    );
    let lines = k.until("close_channel").await;
    assert!(
        lines.contains(&r#"send {"playout_ntp_ms":1234.5,"type":"lmgw.response.stop"}"#.to_owned()),
        "{lines:?}"
    );
    voice.channel_closed(n, "closed".into());
    k.until("close_peer").await;
    assert_eq!(k.ends.lock().unwrap()[0].reason, EndReason::Ended);
}

/// A key refusal at the POST reaches the link (§5.5).
#[tokio::test]
async fn a_key_refusal_at_the_post_stops_the_link() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, _feed) = connected(&mock, json!([])).await;
    mock.state.lock().unwrap().call_answers.push_back((
        401,
        json!({"code": "device_disabled", "message": "device 'phone' is disabled"}).to_string(),
    ));
    let voice = rig.core.voice();
    let (sink, _k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    assert!(voice.offer(n, OFFER.into()).await.is_err());
    rig.connection("refused", |v| v.state == ConnectionState::KeyRefused)
        .await;
    let (sink, _k) = Kotlin::new();
    let err = voice.start(CallStart::default(), sink).await.unwrap_err();
    assert!(matches!(err, CoreError::NotConnected { .. }), "{err:?}");
}

/// The owner hears the call's replies: none notifies while a call of the phone's runs.
#[tokio::test]
async fn no_reply_notifies_while_a_call_runs() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, feed) = connected(&mock, json!([])).await;
    // Without a call a reply notifies (the app is not visible).
    feed.send(&stored(
        "e1:5:bb",
        "message.added",
        reply(7, 30, "It is ten."),
    ));
    rig.next("a reply's notification", |u| {
        matches!(u, Update::Notify { .. })
    })
    .await;
    let voice = rig.core.voice();
    let (sink, _k) = Kotlin::new();
    // The voice task tells the link before it answers the start.
    voice.start(CallStart::default(), sink).await.unwrap();
    feed.send(&stored(
        "e1:6:cc",
        "message.added",
        reply(7, 31, "It is eleven."),
    ));
    assert!(
        rig.within(Duration::from_millis(800), |u| matches!(
            u,
            Update::Notify { .. }
        ))
        .await
        .is_none()
    );
    // The call ends: replies notify again.
    voice.end();
    rig.next(
        "the call's end",
        |u| matches!(u, Update::Voice { view } if view.phase == CallPhase::Ended),
    )
    .await;
    // A round trip through the link's task, which takes the call's end before it answers.
    let _ = rig.core.pairing_preview(String::new()).await;
    feed.send(&stored(
        "e1:7:dd",
        "message.added",
        reply(7, 32, "It is twelve."),
    ));
    rig.next("a reply's notification after the call", |u| {
        matches!(u, Update::Notify { .. })
    })
    .await;
}

/// The assistant overlay's dismissal (§9.2) through the voice task: before the owner's first turn
/// it ends the call it names, without words; after the first turn the call goes on; a dismissal
/// naming another call is dropped.
#[tokio::test]
async fn the_overlay_s_dismissal_ends_its_call_only_before_the_first_turn() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, _feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let created = json!({"type": "session.created", "session": {"type": "realtime",
        "lmgw": {"turn_mode": "automatic", "resolved": {"chat_thread": {"id": 7, "title": "Kai"}}}}})
    .to_string();

    // Before the first turn: the stop, the channel's close, no words.
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    voice.channel_open(n);
    voice.channel_message(n, created.clone());
    k.until("cue").await;
    voice.dismissed(n);
    let request: u64 = k.until("stats").await.last().unwrap()[6..].parse().unwrap();
    voice.stats(n, request, PlayoutStats::default());
    let lines = k.until("close_channel").await;
    assert!(
        lines.contains(&r#"send {"type":"lmgw.response.stop"}"#.to_owned()),
        "{lines:?}"
    );
    voice.channel_closed(n, "closed".into());
    k.until("close_peer").await;
    let end = k.ends.lock().unwrap()[0].clone();
    assert_eq!((end.reason, end.words), (EndReason::Dismissed, None));

    // After the first turn the call goes on; another call's dismissal is dropped.
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    voice.channel_open(n);
    voice.channel_message(n, created);
    k.until("cue").await;
    voice.dismissed(n + 7);
    voice.channel_message(
        n,
        json!({"type": "conversation.item.input_audio_transcription.completed",
            "transcript": "What time is it?"})
        .to_string(),
    );
    rig.next(
        "the first turn",
        |u| matches!(u, Update::Voice { view } if view.spoken && view.phase == CallPhase::Live),
    )
    .await;
    voice.dismissed(n);
    // The owner's End afterwards is the call's first end.
    voice.end();
    let request: u64 = k.until("stats").await.last().unwrap()[6..].parse().unwrap();
    voice.stats(n, request, PlayoutStats::default());
    k.until("close_channel").await;
    assert_eq!(k.ends.lock().unwrap()[0].reason, EndReason::Ended);
}

/// An approval from the desktop's tools in a call (design §11, api §6.4, §15.3): the session's
/// `mcp_approval_request` is a card at once with the ask cue, and one notification while no card is
/// seen, the feed's copy asking nothing more; the verdict goes through the call (one item, then a
/// `response.create` with an `event_id`), never by the route; the resumed turn takes it and the
/// card goes; then a late task result is answered by the continuation once the call is quiet.
#[tokio::test]
async fn an_approval_in_a_call_is_decided_through_the_call_then_a_late_result_continues() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    voice.channel_open(n);
    voice.channel_message(
        n,
        json!({"type": "session.created", "session": {"type": "realtime",
            "lmgw": {"turn_mode": "automatic", "resolved": {"chat_thread": {"id": 7, "title": "Kai"}}}}})
        .to_string(),
    );
    k.until("cue").await;
    // Kai's turn stops on a gated call of the desktop's tools.
    voice.channel_message(
        n,
        json!({"type": "conversation.item.done", "item": {"id": "mcpr_a",
            "type": "mcp_approval_request", "server_label": "desktop", "name": "open_file",
            "arguments": "{\"path\":\"/home/owner/notes.txt\"}"}})
        .to_string(),
    );
    k.until("asked").await;
    rig.next(
        "its notification (the app is not visible)",
        |u| matches!(u, Update::Notify { notification } if notification.id.key == "mcpr_a"),
    )
    .await;
    rig.next("the card", |u| {
        matches!(u, Update::Approvals { open }
        if open.len() == 1 && open[0].in_call && open[0].thread_id == 7)
    })
    .await;
    rig.next("the call waits for the decision", |u| {
        matches!(u, Update::Voice { view }
        if view.awaiting == ["desktop__open_file"]
            && view.words == "Kai waits for your decision on desktop__open_file")
    })
    .await;
    // The feed's copy: its message id on the card, no second notification.
    feed.send(&stored(
        "e1:5:bb",
        "approval.requested",
        requested("mcpr_a", 7, 30),
    ));
    rig.next("the feed's facts", |u| {
        matches!(u, Update::Approvals { open }
        if open.len() == 1 && open[0].message_id == 30 && open[0].in_call)
    })
    .await;
    assert!(
        rig.within(Duration::from_millis(300), |u| matches!(
            u,
            Update::Notify { .. }
        ))
        .await
        .is_none(),
        "asked once"
    );
    // The owner approves: through the call.
    assert_eq!(
        rig.core.decide("mcpr_a".into(), true).await.unwrap(),
        kai_mobile_core::views::Decided::Sent
    );
    let lines = k.until("send {\"event_id\"").await;
    let item = lines
        .iter()
        .find(|l| l.contains("mcp_approval_response"))
        .unwrap_or_else(|| panic!("{lines:?}"));
    let item: serde_json::Value = serde_json::from_str(&item[5..]).unwrap();
    assert_eq!(item["item"]["approval_request_id"], "mcpr_a");
    assert_eq!(item["item"]["approve"], true);
    let create: serde_json::Value = serde_json::from_str(&lines.last().unwrap()[5..]).unwrap();
    assert_eq!(create["type"], "response.create");
    assert!(
        create["event_id"]
            .as_str()
            .unwrap()
            .starts_with("kai_approvals_"),
        "{create}"
    );
    rig.next("the card says sent", |u| {
        matches!(u, Update::Approvals { open }
        if open.len() == 1 && open[0].sent)
    })
    .await;
    // The resumed turn runs; the route was never asked. The phone does not close the card on
    // that: lmgw holds a create while the owner speaks, and the feed says how it decided.
    for e in [
        json!({"type": "response.created", "response": {"id": "r2"}}),
        json!({"type": "output_audio_buffer.started", "response_id": "r2"}),
        json!({"type": "response.done", "response": {"id": "r2", "status": "completed"}}),
        json!({"type": "output_audio_buffer.stopped", "response_id": "r2"}),
    ] {
        voice.channel_message(n, e.to_string());
    }
    assert!(
        rig.within(Duration::from_millis(300), |u| {
            matches!(u, Update::Approvals { open } if open.is_empty())
        })
        .await
        .is_none(),
        "the card stays sent until the feed says"
    );
    feed.send(&stored(
        "e1:6:cc",
        "approval.decided",
        decided_by("mcpr_a", true, PHONE),
    ));
    rig.next(
        "the card went",
        |u| matches!(u, Update::Approvals { open } if open.is_empty()),
    )
    .await;
    assert!(mock.requests("/chat/api/threads/7/approvals").is_empty());
    assert!(
        notices(&rig).is_empty(),
        "decided as sent: {:?}",
        notices(&rig)
    );
    // A task the resumed turn started ends later: the continuation answers it.
    voice.channel_message(
        n,
        json!({"type": "lmgw.task.done", "thread_id": 7, "message_id": 41, "id": 3,
            "task_id": "t3", "server_label": "builder", "tool": "builder__build",
            "status": "completed", "by": null})
        .to_string(),
    );
    let lines = k.until("send {\"event_id\":\"kai_continue_").await;
    let create: serde_json::Value = serde_json::from_str(&lines.last().unwrap()[5..]).unwrap();
    assert_eq!(create["type"], "response.create");
    assert_eq!(
        create.as_object().unwrap().len(),
        2,
        "no new words: {create}"
    );
}

/// A verdict for a call the session showed, given after the call ended, goes by the route; the
/// card the session alone showed went with the call.
#[tokio::test]
async fn after_the_call_its_verdicts_go_by_the_route() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, feed) = connected(&mock, json!([])).await;
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    voice.channel_open(n);
    voice.channel_message(
        n,
        json!({"type": "session.created", "session": {"type": "realtime",
            "lmgw": {"turn_mode": "automatic", "resolved": {"chat_thread": {"id": 7, "title": "Kai"}}}}})
        .to_string(),
    );
    k.until("cue").await;
    voice.channel_message(
        n,
        json!({"type": "conversation.item.done", "item": {"id": "mcpr_b",
            "type": "mcp_approval_request", "server_label": "desktop", "name": "open_file",
            "arguments": "{}"}})
        .to_string(),
    );
    feed.send(&stored(
        "e1:5:bb",
        "approval.requested",
        requested("mcpr_b", 7, 30),
    ));
    rig.next("the card with the feed's facts", |u| {
        matches!(u, Update::Approvals { open }
        if open.len() == 1 && open[0].message_id == 30)
    })
    .await;
    voice.end();
    let request: u64 = k.until("stats").await.last().unwrap()[6..].parse().unwrap();
    voice.stats(n, request, PlayoutStats::default());
    k.until("close_channel").await;
    voice.channel_closed(n, "closed".into());
    rig.next("the card is the feed's now", |u| {
        matches!(u, Update::Approvals { open }
        if open.len() == 1 && !open[0].in_call)
    })
    .await;
    assert_eq!(
        rig.core.decide("mcpr_b".into(), false).await.unwrap(),
        kai_mobile_core::views::Decided::Sent
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.requests("/chat/api/threads/7/approvals").is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the route took the verdict");
}

/// A call bound to thread 7 (automatic) whose session showed `id`, the feed's copy at message 30,
/// and the card for it.
async fn gated(rig: &mut Rig, feed: &super::mock::Feed, id: &str) -> (Arc<Voice>, u64, Kotlin) {
    let voice = rig.core.voice();
    let (sink, mut k) = Kotlin::new();
    let n = voice.start(CallStart::default(), sink).await.unwrap();
    voice.offer(n, OFFER.into()).await.unwrap();
    voice.transport(n, TransportState::Connected, "connected".into());
    voice.channel_open(n);
    voice.channel_message(
        n,
        json!({"type": "session.created", "session": {"type": "realtime",
            "lmgw": {"turn_mode": "automatic", "resolved": {"chat_thread": {"id": 7, "title": "Kai"}}}}})
        .to_string(),
    );
    k.until("cue").await;
    voice.channel_message(
        n,
        json!({"type": "conversation.item.done", "item": {"id": id,
            "type": "mcp_approval_request", "server_label": "desktop", "name": "open_file",
            "arguments": "{}"}})
        .to_string(),
    );
    k.until("asked").await;
    feed.send(&stored(
        "e1:5:bb",
        "approval.requested",
        requested(id, 7, 30),
    ));
    rig.next("the card with the feed's facts", |u| {
        matches!(u, Update::Approvals { open }
        if open.len() == 1 && open[0].message_id == 30 && open[0].in_call)
    })
    .await;
    (voice, n, k)
}

/// The owner approves while still talking (§11): lmgw holds the create for their turn, whose
/// words decline what waits, and the call's response speaks as the resumed one would. The card
/// stays sent until the feed says lmgw declined it on the phone's own new message; then it goes
/// and the owner is told why. The route is never asked.
#[tokio::test]
async fn approve_while_still_talking_the_feed_says_the_owner_s_words_declined_it() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, feed) = connected(&mock, json!([])).await;
    let (voice, n, mut k) = gated(&mut rig, &feed, "mcpr_w").await;
    voice.channel_message(
        n,
        json!({"type": "input_audio_buffer.speech_started"}).to_string(),
    );
    rig.core.decide("mcpr_w".into(), true).await.unwrap();
    k.until("send {\"event_id\":\"kai_approvals_").await;
    for e in [
        json!({"type": "input_audio_buffer.committed"}),
        json!({"type": "conversation.item.input_audio_transcription.completed",
            "item_id": "i1", "transcript": "actually, what time is it"}),
        json!({"type": "response.created", "response": {"id": "r3"}}),
        json!({"type": "output_audio_buffer.started", "response_id": "r3"}),
        json!({"type": "response.done", "response": {"id": "r3", "status": "completed"}}),
        json!({"type": "output_audio_buffer.stopped", "response_id": "r3"}),
    ] {
        voice.channel_message(n, e.to_string());
    }
    assert!(
        rig.within(Duration::from_millis(300), |u| {
            matches!(u, Update::Approvals { open } if open.is_empty())
        })
        .await
        .is_none(),
        "not closed on the call's guess"
    );
    // lmgw wrote the owner's words as a new message, which declined the call, by this phone.
    feed.send(&stored(
        "e1:6:cc",
        "approval.decided",
        decided_by("mcpr_w", false, PHONE),
    ));
    let told = rig
        .next("the owner is told", |u| matches!(u, Update::Notice { .. }))
        .await;
    let Update::Notice { text } = told else {
        unreachable!()
    };
    assert!(
        text.contains("what you said or typed reached lmgw before your approval"),
        "{text}"
    );
    rig.next(
        "the card went",
        |u| matches!(u, Update::Approvals { open } if open.is_empty()),
    )
    .await;
    assert!(mock.requests("/chat/api/threads/7/approvals").is_empty());
}

/// Approve, then End at once (§11): the verdict went through the call and the call ended before
/// lmgw said anything, so the route sends it again; lmgw had decided it in the call, and its 409
/// `approval_decided` closes the card quietly: one decision, no "not taken", no "Already decided".
#[tokio::test]
async fn approve_then_end_at_once_decides_once_and_closes_the_card_quietly() {
    let mock = Mock::start(&PKI, Clients::Required).await;
    let (mut rig, feed) = connected(&mock, json!([])).await;
    let (voice, n, mut k) = gated(&mut rig, &feed, "mcpr_e").await;
    mock.state.lock().unwrap().approvals_answer = Some((
        409,
        json!({"error": {"code": "approval_decided",
            "message": "call 'mcpr_e' was already decided, by device 'phone'"}})
        .to_string(),
    ));
    rig.core.decide("mcpr_e".into(), true).await.unwrap();
    let lines = k.until("send {\"event_id\":\"kai_approvals_").await;
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.contains("mcp_approval_response"))
            .count(),
        1
    );
    voice.end();
    let request: u64 = k.until("stats").await.last().unwrap()[6..].parse().unwrap();
    voice.stats(n, request, PlayoutStats::default());
    k.until("close_channel").await;
    voice.channel_closed(n, "closed".into());
    rig.next(
        "the route's answer closed the card",
        |u| matches!(u, Update::Approvals { open } if open.is_empty()),
    )
    .await;
    let posts = mock.requests("/chat/api/threads/7/approvals");
    assert_eq!(posts.len(), 1, "sent again once");
    let body: serde_json::Value = serde_json::from_slice(&posts[0].body).unwrap();
    assert_eq!(
        body["decisions"],
        json!([{"approval_request_id": "mcpr_e", "approve": true}])
    );
    // The feed's record of the call's decision: as sent, nothing to say.
    feed.send(&stored(
        "e1:6:cc",
        "approval.decided",
        decided_by("mcpr_e", true, PHONE),
    ));
    let _ = rig.within(Duration::from_millis(300), |_| false).await;
    let said = notices(&rig);
    assert!(said.is_empty(), "{said:?}");
    assert!(
        rig.seen
            .iter()
            .all(|u| !matches!(u, Update::Approvals { open }
            if open.iter().any(|a| a.note.is_some()))),
        "never \"not taken\""
    );
}
