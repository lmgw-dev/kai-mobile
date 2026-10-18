//! A core on the host for the tests: a temporary data directory, a software key behind the
//! `Keystore` callback, a clock whose boot time jumps as across a sleep, settings with short waits,
//! and the updates it hands over, read as Kotlin's collector reads them.

use std::{sync::Arc, time::Duration};

use kai_mobile_core::{
    Core, PlatformFacts, Settings,
    clock::ManualClock,
    keystore::{Keystore, soft::SoftKeystore},
    lmgw::requests::{Credentials, confirm_code},
    store::{self, LinkFile},
    views::{ConnectionView, Update},
};

use super::pki::{Pki, device_cert, fp};

pub const ALIAS: &str = "kai-device-test";

pub fn facts() -> PlatformFacts {
    PlatformFacts {
        os: "Android 16".into(),
        app_version: "0.1.0+test".into(),
        notified_approvals: Vec::new(),
        notified_replies: Vec::new(),
    }
}

/// Short waits: a 2 s timeout, a 50 ms stagger, backoffs of 4 s visible and 8 s in the background.
pub fn fast_settings() -> Settings {
    let mut s = Settings::default();
    s.lmgw.timeout_s = 2;
    s.lmgw.url_stagger_ms = 50;
    s.lmgw.retry_max_s = 4;
    s.lmgw.retry_max_background_s = 8;
    s.feed.keepalive_s = 120;
    s
}

pub struct Rig {
    pub dir: tempfile::TempDir,
    pub core: Core,
    pub keystore: Arc<SoftKeystore>,
    pub clock: Arc<ManualClock>,
    /// Every update read so far.
    pub seen: Vec<Update>,
}

/// The pairing a phone keeps after enrolling with `pki`'s gateway at `urls`, its key made in
/// `keystore` under [`ALIAS`]; the certificate valid `years`.
pub fn paired_file(
    pki: &Pki,
    keystore: &SoftKeystore,
    urls: &[&str],
    years: (i32, i32),
) -> LinkFile {
    let spki = keystore.generate(ALIAS.into()).unwrap();
    let cert = device_cert(&pki.ca, &spki, years);
    let to_ms = |y: i32| {
        // 1 January of `y`, UTC.
        let days = (1970..y)
            .map(|y| {
                if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                    366
                } else {
                    365
                }
            })
            .sum::<i64>();
        days * 86_400_000
    };
    LinkFile {
        name: "phone".into(),
        fp: fp(&pki.ca).to_string(),
        alias: ALIAS.into(),
        public_key: kai_mobile_core::lmgw::base64::encode(&spki),
        credentials: Credentials {
            device_id: 12,
            certificate: cert,
            ca_certificate: pki.ca.der().to_vec(),
            not_after_ms: to_ms(years.1),
            notice_after_ms: to_ms(years.1) - 120 * 86_400_000,
            urls: urls
                .iter()
                .map(|u| u.to_string())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            confirm_code: confirm_code(&spki),
        },
        last_good: None,
        cursor: None,
        folder_id: None,
    }
}

impl Rig {
    /// A core with nothing stored.
    pub async fn unpaired() -> Self {
        Self::with(|_, _| None, fast_settings()).await
    }

    /// A core paired with `pki`'s gateway at `urls`.
    pub async fn paired(pki: &Pki, urls: &[&str]) -> Self {
        Self::paired_with(pki, urls, fast_settings()).await
    }

    pub async fn paired_with(pki: &Pki, urls: &[&str], settings: Settings) -> Self {
        let urls: Vec<String> = urls.iter().map(|u| u.to_string()).collect();
        Self::with(
            move |ks, _| {
                let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
                Some(paired_file(pki, ks, &refs, (2020, 2100)))
            },
            settings,
        )
        .await
    }

    /// A core whose data directory holds what `stored` makes (with the keystore and the directory)
    /// and `settings`.
    pub async fn with(
        stored: impl FnOnce(&SoftKeystore, &std::path::Path) -> Option<LinkFile>,
        settings: Settings,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let keystore = SoftKeystore::new();
        if let Some(f) = stored(&keystore, dir.path()) {
            store::write(&dir.path().join(store::LINK_FILE), &f).unwrap();
        }
        store::write(&dir.path().join("settings.json"), &settings).unwrap();
        Self::open(dir, keystore, ManualClock::new())
    }

    /// The same directory, keystore and clock with a new core (a process restart).
    pub fn open(
        dir: tempfile::TempDir,
        keystore: Arc<SoftKeystore>,
        clock: Arc<ManualClock>,
    ) -> Self {
        Self::open_with(dir, keystore, clock, facts())
    }

    /// [`Self::open`] with the system's active notifications at start in `facts`.
    pub fn open_with(
        dir: tempfile::TempDir,
        keystore: Arc<SoftKeystore>,
        clock: Arc<ManualClock>,
        facts: PlatformFacts,
    ) -> Self {
        let core = Core::open(
            dir.path(),
            facts,
            keystore.clone(),
            clock.clone(),
            tokio::runtime::Handle::current(),
        );
        Self {
            dir,
            core,
            keystore,
            clock,
            seen: Vec::new(),
        }
    }

    /// A restart: this core goes, a new one opens the same files.
    pub fn restart(self) -> Self {
        self.restart_with(facts())
    }

    /// A restart with `facts` (the active notifications the shell hands over).
    pub fn restart_with(self, facts: PlatformFacts) -> Self {
        let Rig {
            dir,
            keystore,
            clock,
            ..
        } = self;
        Self::open_with(dir, keystore, clock, facts)
    }

    /// The next update matching `pred`, within 15 s (the rest are kept in [`Self::seen`]).
    pub async fn next(&mut self, what: &str, pred: impl Fn(&Update) -> bool) -> Update {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            let u = tokio::time::timeout_at(deadline, self.core.next_update())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "no update: {what}; the last seen, newest first: {:#?}",
                        self.seen
                            .iter()
                            .rev()
                            .take(16)
                            .map(|u| format!("{u:?}").chars().take(200).collect::<String>())
                            .collect::<Vec<_>>()
                    )
                });
            self.seen.push(u.clone());
            if pred(&u) {
                return u;
            }
        }
    }

    /// Whether an update matching `pred` comes within `wait`.
    pub async fn within(
        &mut self,
        wait: Duration,
        pred: impl Fn(&Update) -> bool,
    ) -> Option<Update> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let u = tokio::time::timeout_at(deadline, self.core.next_update())
                .await
                .ok()?;
            self.seen.push(u.clone());
            if pred(&u) {
                return Some(u);
            }
        }
    }

    /// The next connection view matching `pred`.
    pub async fn connection(
        &mut self,
        what: &str,
        pred: impl Fn(&ConnectionView) -> bool,
    ) -> ConnectionView {
        match self
            .next(
                what,
                |u| matches!(u, Update::Connection { view } if pred(view)),
            )
            .await
        {
            Update::Connection { view } => view,
            _ => unreachable!(),
        }
    }

    /// The stored pairing now.
    pub fn link_file(&self) -> Option<LinkFile> {
        store::read(&self.dir.path().join(store::LINK_FILE)).unwrap()
    }
}

/// `hello` as lmgw sends a device's feed: `open` its open approvals (JSON array text).
pub fn hello(cursor: &str, keepalive_s: u32, open: &str) -> String {
    let h = serde_json::json!({"epoch": "e1", "cursor": cursor, "keepalive_s": keepalive_s,
        "retention_days": 7, "principal": {"kind": "device", "name": "phone"},
        "hold": {"active": false, "fallback_alias": null}, "voice": [], "turns": [],
        "open_approvals": serde_json::from_str::<serde_json::Value>(open).unwrap()});
    format!("event: hello\ndata: {h}\n\n")
}

/// `hello` with the device's facts (device link §7.2).
pub fn hello_with(cursor: &str, urls: &[&str], cert: Option<(&str, &str)>) -> String {
    let h = serde_json::json!({"epoch": "e1", "cursor": cursor, "keepalive_s": 120,
        "retention_days": 7, "principal": {"kind": "device", "name": "phone"},
        "hold": {"active": true, "fallback_alias": "cloud"}, "voice": [], "turns": [],
        "open_approvals": [], "urls": urls,
        "cert": cert.map(|(a, n)| serde_json::json!({"not_after": a, "notice_after": n})),
        "dashboard_url": "http://198.51.100.20:8001/"});
    format!("event: hello\ndata: {h}\n\n")
}

pub fn caught_up(cursor: &str) -> String {
    format!("event: caught_up\ndata: {{\"cursor\":\"{cursor}\"}}\n\n")
}

/// A stored event with its `id:`.
pub fn stored(id: &str, event: &str, data: serde_json::Value) -> String {
    format!("id: {id}\nevent: {event}\ndata: {data}\n\n")
}

/// A live event (no `id:`).
pub fn live(event: &str, data: serde_json::Value) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

/// An `approval.requested` record's data.
pub fn requested(id: &str, thread: i64, message: i64) -> serde_json::Value {
    serde_json::json!({"thread_id": thread, "thread_title": "Kai", "message_id": message,
        "approval_request_id": id, "server_label": "desktop", "name": "open_file",
        "arguments": "{\"path\":\"/tmp/x\"}"})
}

/// An `approval.decided` record's data.
pub fn decided(id: &str, thread: i64, message: i64) -> serde_json::Value {
    serde_json::json!({"thread_id": thread, "message_id": message, "approval_request_id": id,
        "approve": true, "by": "the dashboard"})
}

/// A `message.added` record's data: one assistant message.
pub fn reply(thread: i64, id: i64, text: &str) -> serde_json::Value {
    serde_json::json!({"thread_id": thread, "thread_title": "Kai", "by": "the dashboard",
        "messages": [{"id": id, "thread_id": thread, "role": "assistant", "content": text,
                      "reasoning": "", "kb_refs": [], "attachments": [],
                      "created_at": "2026-10-15 10:00:00"}]})
}
