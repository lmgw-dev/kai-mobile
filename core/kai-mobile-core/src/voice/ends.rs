//! How a call ends, in the owner's words (design §7.7, §7.8): one [`CallEnd`] per way, and the
//! words of `lmgw.session.closed` read through `lmgw-client`'s [`CallClose`] alone (no wait for the
//! feed). `message` is lmgw's sentence, shown under the words and never parsed; only a `revoked`
//! message's leading token is read, by `lmgw-client`, as a 4003's reason is.

use lmgw_client::realtime::{CallClose, CloseKind, RevokeKind};

use super::{
    CallStart,
    view::{CallEnd, EndReason, VoiceAction},
};

fn end(reason: EndReason, words: Option<String>, actions: Vec<VoiceAction>) -> CallEnd {
    CallEnd {
        reason,
        words,
        message: None,
        by: None,
        actions,
        restart: None,
    }
}

/// The owner's End: no words.
pub fn owner() -> CallEnd {
    end(EndReason::Ended, None, Vec::new())
}

/// End while the bind was pending (§7.9): no words, nothing was bound.
pub fn declined() -> CallEnd {
    end(EndReason::Declined, None, Vec::new())
}

/// The assistant overlay dismissed before the owner's first turn (§7.7, §9.2): no words, an
/// accidental invocation leaves nothing open.
pub fn dismissed() -> CallEnd {
    end(EndReason::Dismissed, None, Vec::new())
}

/// `voice.idle_end_s` of quiet (§7.7); a pending bind counts as quiet.
pub fn idle(idle_end_s: u32) -> CallEnd {
    end(
        EndReason::Idle,
        Some(format!(
            "voice ended after {idle_end_s} s of quiet (voice.idle_end_s)"
        )),
        Vec::new(),
    )
}

/// The conversation moved to another thread (§7.7's rebind): no words, a new call at once.
pub fn rebind(restart: CallStart) -> CallEnd {
    CallEnd {
        restart: Some(restart),
        ..end(EndReason::Rebind, None, Vec::new())
    }
}

/// The transport failed, or the channel closed without `lmgw.session.closed` (§7.8's last row).
pub fn connection_lost(state: &str) -> CallEnd {
    end(
        EndReason::ConnectionLost,
        Some(format!("connection lost ({state})")),
        vec![VoiceAction::Reconnect],
    )
}

/// No `session.created` nor `bind_pending` within `voice.connect_timeout_s` (§7.3 step 6).
pub fn connect_timeout(connect_timeout_s: u32, cue: bool) -> CallEnd {
    let mut words = format!(
        "lmgw did not start the voice session within {connect_timeout_s} s \
         (voice.connect_timeout_s)"
    );
    if cue {
        words.push_str("; nothing said since the listening cue was answered");
    }
    end(
        EndReason::ConnectTimeout,
        Some(words),
        vec![VoiceAction::Reconnect],
    )
}

/// A permanent loss of the audio focus (§7.7, §8.5): another app took the audio.
pub fn audio_taken() -> CallEnd {
    end(
        EndReason::AudioTaken,
        Some("voice ended: another app took the audio".to_owned()),
        Vec::new(),
    )
}

/// The phone could not carry the call on (libwebrtc, Android), in Kotlin's words.
pub fn failed(words: String) -> CallEnd {
    end(EndReason::Failed, Some(words), vec![VoiceAction::Reconnect])
}

/// The call could not be set up: the POST's refusal or failure, in its own words.
pub fn refused(words: String, reconnect: bool) -> CallEnd {
    end(
        EndReason::Refused,
        Some(words),
        if reconnect {
            vec![VoiceAction::Reconnect]
        } else {
            Vec::new()
        },
    )
}

/// A revocation's words by its kind (§5.5); a token this build does not know says that lmgw refused
/// the device, until the feed's `revoked` or the next request's 401 names the kind.
pub fn revoked_words(kind: &RevokeKind) -> String {
    match kind {
        RevokeKind::CertificateExpired => "This device's certificate lapsed: pair it again (New \
                                           token… on lmgw's Devices card)."
            .to_owned(),
        RevokeKind::DeviceDisabled => {
            "This device was disabled on lmgw: enable it on its Devices card, or pair again."
                .to_owned()
        }
        RevokeKind::KeyExpired => "This device's access expired on lmgw: move its expiry on its \
                                   Devices card, or pair again."
            .to_owned(),
        RevokeKind::KeyUnknown | RevokeKind::Revoked => {
            "lmgw no longer knows this device's key (paired again elsewhere, or deleted): pair \
             again."
                .to_owned()
        }
        _ => "lmgw refused this device".to_owned(),
    }
}

/// `lmgw.session.closed`'s words (§7.8's table). `refusal`: an `error` that came just before it
/// while the call was not bound (a refusal at the open, calls C21), shown with its code. `kind` is
/// the event's own token, for the log of a kind this build does not know.
pub fn closed(c: &CallClose, kind: &str, refusal: Option<&str>) -> CallEnd {
    let by = c.by.clone();
    let who = by.clone().unwrap_or_else(|| "another client".to_owned());
    let (reason, words, actions) = match &c.kind {
        CloseKind::TakenOver => (
            EndReason::TakenOver,
            format!("Voice moved to {who}"),
            vec![VoiceAction::TakeOver],
        ),
        CloseKind::ThreadBound => (
            EndReason::ThreadBound,
            format!("Voice stayed on {who}"),
            vec![VoiceAction::TakeOver],
        ),
        CloseKind::Revoked { kind } => (EndReason::Revoked, revoked_words(kind), Vec::new()),
        CloseKind::OutOfReach => (
            EndReason::OutOfReach,
            "the conversation's thread is gone".to_owned(),
            vec![VoiceAction::Reconnect],
        ),
        CloseKind::ShuttingDown => (
            EndReason::ShuttingDown,
            // lmgw sends the same kind and sentence for its stop and for a device listener that
            // changed (calls §8.2, as built): the connection's state line tells which.
            "lmgw is shutting down, or its device access changed".to_owned(),
            vec![VoiceAction::Reconnect],
        ),
        CloseKind::Other { .. } if kind == "closed" => (
            EndReason::Closed,
            match refusal {
                Some(r) => format!("lmgw refused the voice session: {r}"),
                None => "lmgw ended the voice session".to_owned(),
            },
            vec![VoiceAction::Reconnect],
        ),
        _ => {
            tracing::info!(%kind, "voice: lmgw ended the call with a kind this build does not know");
            (
                EndReason::Other,
                "lmgw ended the voice session".to_owned(),
                vec![VoiceAction::Reconnect],
            )
        }
    };
    CallEnd {
        reason,
        words: Some(words),
        message: (!c.message.is_empty()).then(|| c.message.clone()),
        by,
        actions,
        restart: None,
    }
}
