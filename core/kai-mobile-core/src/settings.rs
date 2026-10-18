//! The settings (design §3.4): typed, with the defaults of the design's table, kept in the core's
//! JSON file. Grouped by the first part of each key (`lmgw.timeout_s` is `settings.lmgw.timeout_s`),
//! and every group and field falls back to its default when the file lacks it, so a newer app
//! reads an older file. A value outside what it can be is an error that names the key; nothing is
//! clamped silently (no hidden limits).

use serde::{Deserialize, Serialize};

use crate::CoreError;

/// Every setting. `Settings::default()` is the design's table.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct Settings {
    pub lmgw: LmgwSettings,
    pub feed: FeedSettings,
    pub pairing: PairingSettings,
    pub conversation: ConversationSettings,
    pub voice: VoiceSettings,
    pub appearance: AppearanceSettings,
}

/// `lmgw.*`: how the core reaches the gateway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct LmgwSettings {
    /// A connect (the whole URL race), an answer's head, the feed's first byte, in seconds.
    pub timeout_s: u32,
    /// The wait before the race tries the next URL, in milliseconds (RFC 8305's attempt delay).
    pub url_stagger_ms: u32,
    /// The feed's backoff ceiling while the app is visible, in seconds.
    pub retry_max_s: u32,
    /// The feed's backoff ceiling while the app is not visible, in seconds.
    pub retry_max_background_s: u32,
}

impl Default for LmgwSettings {
    fn default() -> Self {
        Self {
            timeout_s: 10,
            url_stagger_ms: 250,
            retry_max_s: 30,
            retry_max_background_s: 300,
        }
    }
}

/// `feed.*`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct FeedSettings {
    /// The keep-alive the held feed asks of lmgw, in seconds.
    pub keepalive_s: u32,
}

impl Default for FeedSettings {
    fn default() -> Self {
        Self { keepalive_s: 120 }
    }
}

/// `pairing.*`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct PairingSettings {
    /// How often the core asks again while it waits for the owner's Confirm, in seconds.
    pub confirm_poll_s: u32,
}

impl Default for PairingSettings {
    fn default() -> Self {
        Self { confirm_poll_s: 2 }
    }
}

/// `conversation.*`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct ConversationSettings {
    /// The name of the Chat folder the Kai conversation lives in.
    pub folder_name: String,
}

impl Default for ConversationSettings {
    fn default() -> Self {
        Self {
            folder_name: "Kai".to_owned(),
        }
    }
}

/// `voice.mode`: whose turn-taking the call follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMode {
    /// The thread's mode as lmgw reports it (`session.lmgw.turn_mode`).
    #[default]
    Thread,
    /// Always automatic.
    Automatic,
    /// Always push-to-talk.
    PushToTalk,
}

/// `voice.echo` (design §8.2): the device module's canceller, and whether the session runs half
/// duplex. A change applies from the next call: the factory and its device module are rebuilt
/// between calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum EchoMode {
    /// The hardware canceller where the device reports one, else software.
    #[default]
    Auto,
    /// The phone's hardware canceller and noise suppressor.
    Hardware,
    /// WebRTC's own (AEC3), the hardware's off.
    Software,
    /// As `Software` on the phone, and the session runs half duplex
    /// (`session.lmgw.half_duplex: true` with `voice.echo_tail_ms`): speech does not interrupt
    /// Kai, the tap does.
    HalfDuplex,
}

/// `voice.route`: the default output when no headset is connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    #[default]
    Speaker,
    Earpiece,
}

/// `voice.*`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct VoiceSettings {
    pub mode: VoiceMode,
    /// From the trigger to `session.created` or a pending bind, in seconds.
    pub connect_timeout_s: u32,
    /// Seconds of quiet before a call ends; 0 = never.
    pub idle_end_s: u32,
    /// How often Kai's playout is reported to lmgw while its audio plays, in milliseconds.
    pub playout_report_ms: u32,
    /// How long the track stays on after a push-to-talk release, in milliseconds. `None` until
    /// WP0 (g) measures it: no value is guessed in its place.
    pub ptt_tail_ms: Option<u32>,
    pub echo: EchoMode,
    /// Sent with half duplex. `None` = the dashboard's value.
    pub echo_tail_ms: Option<u32>,
    pub route: Route,
    pub earcon: bool,
    pub haptics: bool,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            mode: VoiceMode::default(),
            connect_timeout_s: 15,
            idle_end_s: 30,
            playout_report_ms: 1000,
            ptt_tail_ms: None,
            echo: EchoMode::default(),
            echo_tail_ms: None,
            route: Route::default(),
            earcon: true,
            haptics: true,
        }
    }
}

/// `appearance.colours`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Colours {
    /// Kai's graphite and blue.
    #[default]
    Kai,
    /// The system's dynamic colour.
    System,
}

/// `appearance.*`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct AppearanceSettings {
    pub colours: Colours,
}

impl Settings {
    /// Checks every value; the first one outside what it can be names its key.
    pub fn validate(&self) -> Result<(), CoreError> {
        let positive = |key: &str, v: u32| -> Result<(), CoreError> {
            if v == 0 {
                Err(CoreError::InvalidSetting {
                    key: key.to_owned(),
                    reason: "must be at least 1".to_owned(),
                })
            } else {
                Ok(())
            }
        };
        positive("lmgw.timeout_s", self.lmgw.timeout_s)?;
        positive("lmgw.retry_max_s", self.lmgw.retry_max_s)?;
        positive(
            "lmgw.retry_max_background_s",
            self.lmgw.retry_max_background_s,
        )?;
        if self.lmgw.retry_max_background_s < self.lmgw.retry_max_s {
            return Err(CoreError::InvalidSetting {
                key: "lmgw.retry_max_background_s".to_owned(),
                reason: "must not be below lmgw.retry_max_s".to_owned(),
            });
        }
        positive("feed.keepalive_s", self.feed.keepalive_s)?;
        positive("pairing.confirm_poll_s", self.pairing.confirm_poll_s)?;
        positive("voice.connect_timeout_s", self.voice.connect_timeout_s)?;
        positive("voice.playout_report_ms", self.voice.playout_report_ms)?;
        if self.conversation.folder_name.trim().is_empty() {
            return Err(CoreError::InvalidSetting {
                key: "conversation.folder_name".to_owned(),
                reason: "must not be empty".to_owned(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_design_table() {
        let s = Settings::default();
        assert_eq!(
            s.lmgw,
            LmgwSettings {
                timeout_s: 10,
                url_stagger_ms: 250,
                retry_max_s: 30,
                retry_max_background_s: 300
            }
        );
        assert_eq!(s.feed.keepalive_s, 120);
        assert_eq!(s.pairing.confirm_poll_s, 2);
        assert_eq!(s.conversation.folder_name, "Kai");
        assert_eq!(s.voice.connect_timeout_s, 15);
        assert_eq!(s.voice.idle_end_s, 30);
        assert_eq!(s.voice.playout_report_ms, 1000);
        assert_eq!(s.voice.ptt_tail_ms, None);
        assert_eq!(s.voice.route, Route::Speaker);
        assert!(s.voice.earcon && s.voice.haptics);
        assert_eq!(s.appearance.colours, Colours::Kai);
        s.validate().unwrap();
    }

    #[test]
    fn an_older_file_reads_with_defaults_for_what_it_lacks() {
        let s: Settings =
            serde_json::from_str(r#"{"lmgw":{"timeout_s":20},"voice":{"mode":"push_to_talk"}}"#)
                .unwrap();
        assert_eq!(s.lmgw.timeout_s, 20);
        assert_eq!(s.lmgw.url_stagger_ms, 250);
        assert_eq!(s.voice.mode, VoiceMode::PushToTalk);
        assert_eq!(s.feed, FeedSettings::default());
    }

    #[test]
    fn the_echo_setting_reads_and_writes_its_four_values() {
        for (text, mode) in [
            ("auto", EchoMode::Auto),
            ("hardware", EchoMode::Hardware),
            ("software", EchoMode::Software),
            ("half_duplex", EchoMode::HalfDuplex),
        ] {
            let s: Settings =
                serde_json::from_str(&format!(r#"{{"voice":{{"echo":"{text}"}}}}"#)).unwrap();
            assert_eq!(s.voice.echo, mode);
            let back = serde_json::to_value(&s).unwrap();
            assert_eq!(back["voice"]["echo"], text);
        }
        assert_eq!(Settings::default().voice.echo, EchoMode::Auto);
    }

    #[test]
    fn a_value_out_of_range_names_its_key() {
        let mut s = Settings::default();
        s.feed.keepalive_s = 0;
        let CoreError::InvalidSetting { key, .. } = s.validate().unwrap_err() else {
            panic!()
        };
        assert_eq!(key, "feed.keepalive_s");
        let mut s = Settings::default();
        s.lmgw.retry_max_background_s = 5;
        let CoreError::InvalidSetting { key, .. } = s.validate().unwrap_err() else {
            panic!()
        };
        assert_eq!(key, "lmgw.retry_max_background_s");
    }
}
