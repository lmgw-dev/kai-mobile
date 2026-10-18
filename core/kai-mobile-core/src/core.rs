//! The `Core` object Kotlin holds for the process's life (design §3.2): its version, the lmgw pin,
//! the settings, and every entry point of the link (pairing, the held feed, the conversation,
//! approvals, notifications). Each entry point sends the link's actor a message (a synchronous one
//! returns at once, an async one awaits the answer), so none waits behind a lock.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock, atomic::AtomicBool},
    time::Duration,
};

use arc_swap::ArcSwap;
use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot},
};

use crate::{
    CoreError, LmgwPin, Settings,
    clock::{Clock, SystemClock},
    conversation::{load, send},
    keystore::Keystore,
    link::actor::{Actor, Cmd, Ctx, spawn_verdicts},
    logging,
    pairing::{self, Enroller},
    views::{Decided, NetworkFacts, NotificationId, PairingPreview, PairingView, Update},
    voice::{
        Voice,
        actor::{Cmd as VoiceCmd, Ctx as VoiceCtx, VoiceActor},
    },
};

/// Where the core keeps its files: the app's internal storage, named by Kotlin.
#[derive(Debug, Clone, uniffi::Record)]
pub struct AppDirs {
    /// Files that must survive (`Context.filesDir`): the settings, `link.json`, `approvals.json`.
    pub data_dir: String,
    /// Files that may be dropped (`Context.cacheDir`).
    pub cache_dir: String,
}

/// What the platform says about itself (design §14: Android facts enter here).
#[derive(Debug, Clone, uniffi::Record)]
pub struct PlatformFacts {
    /// The OS and its release, for the enrolment's `client` text ("Android 16").
    pub os: String,
    /// The app's version name.
    pub app_version: String,
    /// The approval ids of the system's active notifications at start (§5.4: the notified ids
    /// outlive the process, and the first feed's reader must know them).
    pub notified_approvals: Vec<String>,
    /// The threads of the active reply notifications at start.
    pub notified_replies: Vec<i64>,
}

/// What the About page shows of the core.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreAbout {
    /// The core crate's version.
    pub version: String,
    /// The lmgw source the core is built against.
    pub lmgw_pin: LmgwPin,
    /// `debug` or `release`.
    pub profile: String,
    /// The target the library was built for, e.g. `aarch64-linux-android`.
    pub target: String,
}

const SETTINGS_FILE: &str = "settings.json";

/// The process's one runtime (design §3.1): multi-threaded, the default worker count, kept for the
/// process's life.
fn runtime() -> Handle {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("kai-core")
                .build()
                .expect("the core's tokio runtime starts")
        })
        .handle()
        .clone()
}

/// The core. Created once, by Kotlin.
#[derive(uniffi::Object)]
pub struct Core {
    rt: Handle,
    settings_path: PathBuf,
    /// Read lock-free by `settings()` and the actor; replaced whole by `set_settings`.
    settings: Arc<ArcSwap<Settings>>,
    /// Serialises the writes of the settings file.
    write: tokio::sync::Mutex<()>,
    /// Why the settings file was set aside at start, if it was.
    settings_notice: Option<String>,
    cmds: mpsc::UnboundedSender<Cmd>,
    updates: tokio::sync::Mutex<mpsc::UnboundedReceiver<Update>>,
    enroller: Arc<Enroller>,
    facts: PlatformFacts,
    paired: Arc<AtomicBool>,
    voice: Arc<Voice>,
    voice_cmds: mpsc::UnboundedSender<VoiceCmd>,
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.cmds.send(Cmd::Shutdown);
        let _ = self.voice_cmds.send(VoiceCmd::Shutdown);
    }
}

impl Core {
    /// A core over `data_dir` on `rt` with `clock` (the host tests' constructor; the FFI one uses
    /// the process's runtime and the system's clocks).
    pub fn open(
        data_dir: &Path,
        facts: PlatformFacts,
        keystore: Arc<dyn Keystore>,
        clock: Arc<dyn Clock>,
        rt: Handle,
    ) -> Self {
        logging::init();
        if let Err(e) = fs::create_dir_all(data_dir) {
            // The writes will fail with their own errors; the core still starts.
            tracing::error!(dir = %data_dir.display(), error = %e, "cannot create the data directory");
        }
        let settings_path = data_dir.join(SETTINGS_FILE);
        let (settings, settings_notice) = match load(&settings_path) {
            Ok(s) => (s, None),
            Err(note) => (Settings::default(), Some(note)),
        };
        let settings = Arc::new(ArcSwap::from_pointee(settings));
        let (cmds, rx) = mpsc::unbounded_channel();
        let (updates_tx, updates) = mpsc::unbounded_channel();
        let (voice_cmds, voice_rx) = mpsc::unbounded_channel();
        let paired = Arc::new(AtomicBool::new(false));
        let voice_actor = VoiceActor::new(
            VoiceCtx {
                clock: clock.clone(),
                settings: settings.clone(),
                updates: updates_tx.clone(),
                link: cmds.clone(),
                cmds: voice_cmds.clone(),
                data_dir: data_dir.to_path_buf(),
            },
            voice_rx,
        );
        let ctx = Ctx {
            clock,
            keystore: keystore.clone(),
            settings: settings.clone(),
            data_dir: data_dir.to_path_buf(),
            updates: updates_tx,
            cmds: cmds.clone(),
            paired: paired.clone(),
            voice: voice_cmds.clone(),
        };
        let actor = Actor::new(
            ctx,
            rx,
            facts.notified_approvals.clone(),
            facts.notified_replies.clone(),
        );
        rt.spawn(actor.run());
        rt.spawn(voice_actor.run());
        let voice = Arc::new(Voice::new(voice_cmds.clone(), cmds.clone()));
        tracing::info!(version = env!("CARGO_PKG_VERSION"), "core started");
        Self {
            rt,
            settings_path,
            settings,
            write: tokio::sync::Mutex::new(()),
            settings_notice,
            cmds,
            updates: tokio::sync::Mutex::new(updates),
            enroller: Arc::new(Enroller::new(keystore)),
            facts,
            paired,
            voice,
            voice_cmds,
        }
    }

    /// Runs `f` on the core's runtime and waits for it (the FFI's futures are polled elsewhere).
    async fn on_rt<T: Send + 'static>(&self, f: impl Future<Output = T> + Send + 'static) -> T {
        match self.rt.spawn(f).await {
            Ok(t) => t,
            Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
            Err(e) => panic!("a core task was cancelled: {e}"),
        }
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.cmds.send(cmd);
    }

    async fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> Cmd) -> Option<T> {
        let (tx, rx) = oneshot::channel();
        self.send(make(tx));
        rx.await.ok()
    }

    /// The Devices card's text for this client ("Kai mobile 0.1.0 (Android 16)").
    fn client_text(&self) -> String {
        format!("Kai mobile {} ({})", self.facts.app_version, self.facts.os)
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(u64::from(self.settings.load().lmgw.timeout_s))
    }
}

/// Reads the settings file. A file that does not exist is the defaults; a file that cannot be read
/// or parsed is moved aside under a timestamped name (never deleted, never overwritten) and the reason returned.
fn load(path: &Path) -> Result<Settings, String> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(e) => {
            return Err(format!(
                "{} could not be read ({e}); the defaults are in use",
                path.display()
            ));
        }
    };
    let parsed = serde_json::from_str::<Settings>(&text)
        .map_err(|e| e.to_string())
        .and_then(|s| s.validate().map(|()| s).map_err(|e| e.to_string()));
    parsed.map_err(|why| {
        // A name of its own per rejection, so an earlier rejected file is never overwritten.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let aside = path.with_extension(format!("json.rejected-{stamp}"));
        match fs::rename(path, &aside) {
            Ok(()) => format!("{} was not usable ({why}); it was moved to {} and the defaults are in use", path.display(), aside.display()),
            Err(e) => format!("{} was not usable ({why}) and could not be moved aside ({e}); the defaults are in use", path.display()),
        }
    })
}

#[uniffi::export(async_runtime = "tokio")]
impl Core {
    #[uniffi::constructor]
    pub fn new(dirs: AppDirs, facts: PlatformFacts, keystore: Box<dyn Keystore>) -> Arc<Self> {
        let rt = runtime();
        let _guard = rt.enter();
        Arc::new(Self::open(
            Path::new(&dirs.data_dir),
            facts,
            Arc::from(keystore),
            Arc::new(SystemClock),
            rt.clone(),
        ))
    }

    /// The core crate's version.
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    /// The lmgw source this core is built against.
    pub fn lmgw_pin(&self) -> LmgwPin {
        LmgwPin::built_in()
    }

    /// Everything the About page shows of the core.
    pub fn about(&self) -> CoreAbout {
        CoreAbout {
            version: self.version(),
            lmgw_pin: self.lmgw_pin(),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .to_owned(),
            target: target_triple(),
        }
    }

    /// The settings as they stand.
    pub fn settings(&self) -> Settings {
        (**self.settings.load()).clone()
    }

    /// Why the settings file was set aside at start, for the settings page to show.
    pub fn settings_notice(&self) -> Option<String> {
        self.settings_notice.clone()
    }

    /// Validates, writes the file atomically (a temp file and rename) and only then takes the new
    /// settings into use, so a failed write leaves the old ones in force and says so.
    pub async fn set_settings(&self, settings: Settings) -> Result<(), CoreError> {
        settings.validate()?;
        let _writing = self.write.lock().await;
        let storage = |e: std::io::Error| CoreError::Storage {
            what: "settings".to_owned(),
            reason: e.to_string(),
        };
        let json = serde_json::to_vec_pretty(&settings).map_err(|e| CoreError::Storage {
            what: "settings".to_owned(),
            reason: e.to_string(),
        })?;
        let tmp = self.settings_path.with_extension("json.tmp");
        {
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::File::create(&tmp).await.map_err(storage)?;
            file.write_all(&json).await.map_err(storage)?;
            file.sync_all().await.map_err(storage)?;
        }
        tokio::fs::rename(&tmp, &self.settings_path)
            .await
            .map_err(storage)?;
        self.settings.store(Arc::new(settings));
        self.send(Cmd::SettingsChanged);
        Ok(())
    }

    /// Whether a pairing is stored (the boot receiver starts Kai's service by it).
    pub fn paired(&self) -> bool {
        self.paired.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The link `text` parsed (strict v2), with what the owner confirms (§4.1); no network.
    pub async fn pairing_preview(&self, text: String) -> Result<PairingPreview, CoreError> {
        let (current, _) = self
            .ask(|reply| Cmd::Current { reply })
            .await
            .unwrap_or_default();
        pairing::preview(&text, current.as_deref()).map(|(_, p)| p)
    }

    /// Pairs with the link `text` (§4.2): the Keystore key (the same one for the same token),
    /// enrolment raced on the link's URLs over TLS pinned to its `fp`, the pairing stored, the old
    /// one's key deleted, the feed started. Kotlin drops the text afterwards.
    pub async fn pair(&self, text: String) -> Result<PairingView, CoreError> {
        let (current, alias) = self
            .ask(|reply| Cmd::Current { reply })
            .await
            .unwrap_or_default();
        let (link, _) = pairing::preview(&text, current.as_deref())?;
        drop(text);
        let s = self.settings.load_full();
        let timeout = Duration::from_secs(u64::from(s.lmgw.timeout_s));
        let stagger = Duration::from_millis(u64::from(s.lmgw.url_stagger_ms));
        let client = self.client_text();
        let enroller = self.enroller.clone();
        let cmds = self.cmds.clone();
        self.on_rt(async move {
            let enrolled = enroller
                .enrol(&link, &client, timeout, stagger, alias.as_deref())
                .await?;
            let (reply, rx) = oneshot::channel();
            let _ = cmds.send(Cmd::Paired {
                enrolled: Box::new(enrolled),
                name: link.name.clone(),
                fp: link.fp.to_string(),
                reply,
            });
            rx.await.unwrap_or_else(|_| Err(stopped()))
        })
        .await
    }

    /// Forget gateway (§3.3): `link.json`, `approvals.json` and the Keystore key go, the feed stops,
    /// every notification of it is withdrawn.
    pub async fn forget(&self) {
        let _ = self.ask(|reply| Cmd::Forget { reply }).await;
        self.enroller.clear().await;
    }

    /// Check now / Try again: one try at once, the backoff reset.
    pub fn retry(&self) {
        self.send(Cmd::Retry);
    }

    /// The default network changed (§5.2).
    pub fn set_network(&self, net: NetworkFacts) {
        self.send(Cmd::Network(net));
    }

    /// The app became visible or left the screen (§6.1, §12).
    pub fn set_foreground(&self, visible: bool) {
        self.send(Cmd::Foreground(visible));
    }

    /// The assistant overlay shows the approval cards (it shows, unlocked) or no longer does
    /// (§9.2, §11). With the app's visibility it says whether a card is seen: while one is, no
    /// approval notification asks as well, and when none is any more the calls still waiting are
    /// notified.
    pub fn set_overlay(&self, showing_cards: bool) {
        self.send(Cmd::Overlay(showing_cards));
    }

    /// The alarm fired, or the process woke: the deadlines are compared with the boot clock
    /// (§12.2).
    pub fn wake(&self) {
        self.send(Cmd::Wake);
    }

    /// The next update (§3.2), pulled into a Flow by one collector. Cancellation-safe: polled
    /// in the caller's future (no task of its own), so a collector cancelled while it waits loses
    /// no update.
    pub async fn next_update(&self) -> Update {
        match self.updates.lock().await.recv().await {
            Some(u) => u,
            None => std::future::pending().await,
        }
    }

    /// Sends `text` to the Kai thread (§6.3) and reads the reply's stream to its end; the reply
    /// streams in through `Update::Messages`. Answers at once with the state's words when the phone
    /// is not connected or the folder is not ready.
    pub async fn send_text(&self, text: String) -> Result<(), CoreError> {
        let (h, thread) = self
            .ask(|reply| Cmd::SendStart { reply })
            .await
            .unwrap_or_else(|| Err(stopped()))?;
        let timeout = self.timeout();
        let cmds = self.cmds.clone();
        self.on_rt(async move {
            let req = send::send_request(thread, &text);
            let frames = cmds.clone();
            let result = send::stream(&h, &req, timeout, |f| {
                let _ = frames.send(Cmd::SendFrame(f));
            })
            .await;
            let out = match &result {
                Ok(()) => Ok(()),
                Err(send::StreamFail::Refused { error, status }) => Err(CoreError::Refused {
                    code: error.code.clone(),
                    reason: error.message.clone(),
                    hint: (*status == 429 && error.code == "key_rate").then(|| {
                        "a voice call holds one of this device's concurrency slots for its life"
                            .to_owned()
                    }),
                }),
                Err(send::StreamFail::NoAnswer(n)) => Err(CoreError::Unreachable {
                    reason: n.words(timeout),
                    setting: Some("lmgw.timeout_s".to_owned()),
                }),
                Err(send::StreamFail::Broke(why)) => Err(CoreError::Unreachable {
                    reason: format!("the reply broke off: {why}"),
                    setting: None,
                }),
            };
            let _ = cmds.send(Cmd::SendEnded {
                identity: h.identity(),
                result,
            });
            out
        })
        .await
    }

    /// New conversation (§6.1): `current` with `{new: true}`; the conversation moves to the new
    /// thread.
    pub async fn new_conversation(&self) -> Result<(), CoreError> {
        let (h, name, remembered, generation) = self
            .ask(|reply| Cmd::NewStart { reply })
            .await
            .unwrap_or_else(|| Err(stopped()))?;
        let timeout = self.timeout();
        let cmds = self.cmds.clone();
        self.on_rt(async move {
            let loaded = load::load(&h, &name, remembered, true, timeout).await;
            let out = match &loaded {
                load::Loaded::Ready { .. } => Ok(()),
                load::Loaded::NotReady { steps, .. } => Err(CoreError::NotReady {
                    reason: steps.clone(),
                }),
                load::Loaded::Failed(load::Fail::Refused { error, .. }) => {
                    Err(CoreError::Refused {
                        code: error.code.clone(),
                        reason: error.message.clone(),
                        hint: None,
                    })
                }
                load::Loaded::Failed(load::Fail::NoAnswer(n)) => Err(CoreError::Unreachable {
                    reason: n.words(timeout),
                    setting: Some("lmgw.timeout_s".to_owned()),
                }),
            };
            let _ = cmds.send(Cmd::Loaded {
                generation,
                identity: h.identity(),
                loaded: Box::new(loaded),
                probe: false,
            });
            out
        })
        .await
    }

    /// A verdict for a waiting call (§11): held until every call of its reply has one, then sent
    /// together; the resumed turn's stream is read to its end in the background (a request that
    /// stopped reading would stop the turn).
    pub async fn decide(
        &self,
        approval_request_id: String,
        approve: bool,
    ) -> Result<Decided, CoreError> {
        let (decided, send) = self
            .ask(|reply| Cmd::Decide {
                id: approval_request_id,
                approve,
                reply,
            })
            .await
            .unwrap_or_else(|| Err(stopped()))?;
        let Some((h, thread, verdicts)) = send else {
            return Ok(decided);
        };
        let timeout = self.timeout();
        let cmds = self.cmds.clone();
        let _guard = self.rt.enter();
        spawn_verdicts(h, thread, verdicts, timeout, cmds, false);
        Ok(decided)
    }

    /// The user dismissed an approval's notification: it stays dismissed across reconnects
    /// (`approvals.json`). Answers once `approvals.json` holds it (the shell's receiver finishes
    /// only then), or with why it could not be written.
    pub async fn dismiss_approval(&self, approval_request_id: String) -> Result<(), CoreError> {
        self.ask(|reply| Cmd::Dismiss {
            id: approval_request_id,
            reply,
        })
        .await
        .unwrap_or_else(|| Err(stopped()))
    }

    /// The voice entry points (design §3.2, §7).
    pub fn voice(&self) -> Arc<Voice> {
        self.voice.clone()
    }

    /// A notification is gone by another way (tapped and cancelled with it).
    pub fn notification_gone(&self, id: NotificationId) {
        self.send(Cmd::Gone {
            kind: id.kind,
            key: id.key,
        });
    }
}

fn stopped() -> CoreError {
    CoreError::Storage {
        what: "the core".to_owned(),
        reason: "the core stopped".to_owned(),
    }
}

fn target_triple() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    match arch {
        "aarch64" if os == "android" => "aarch64-linux-android".to_owned(),
        "x86_64" if os == "android" => "x86_64-linux-android".to_owned(),
        _ => format!("{arch}-{os}"),
    }
}
