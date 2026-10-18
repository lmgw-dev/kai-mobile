//! A turn's `error` frames as the conversation shows them (design §6.3). A frame about an MCP
//! label the thread or its folder attached (`LabelCode`) is a notice: the turn went on without
//! that label's tools, so the line stays under the saved answer. Any other frame is the turn's
//! error. The code decides, never the message's text; the device's name is read out of the
//! message only to name it in the offline notice.

use lmgw_client::requests::{ErrorFrame, LabelCode};

/// What a frame is to the conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Said {
    /// A label's notice, in the app's words.
    Notice(String),
    /// The turn's error, verbatim with its code.
    Error(String),
}

/// `frame` as the conversation shows it.
pub fn said(frame: &ErrorFrame) -> Said {
    let Some(code) = frame.label_code() else {
        return Said::Error(match &frame.code {
            Some(c) => format!("{} ({c})", frame.message),
            None => frame.message.clone(),
        });
    };
    let label = frame.server_label.as_deref().unwrap_or("a tool server");
    Said::Notice(match code {
        LabelCode::Offline => {
            format!(
                "{} is offline. Answered without its tools.",
                device_name(&frame.message).unwrap_or(label)
            )
        }
        LabelCode::Unreachable => format!(
            "This phone may not use {label}'s tools. Grant it under the phone's policy, Device \
             tools, in lmgw's dashboard."
        ),
        LabelCode::Disabled | LabelCode::Unknown | LabelCode::Failed => frame.message.clone(),
    })
}

/// The device named by lmgw's offline words (`... device 'Kai desktop' is offline ...`).
fn device_name(message: &str) -> Option<&str> {
    let rest = message.split_once("device '")?.1;
    let name = rest.split_once('\'')?.0;
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(code: Option<&str>, label: Option<&str>, message: &str) -> ErrorFrame {
        ErrorFrame {
            message: message.into(),
            code: code.map(Into::into),
            server_label: label.map(Into::into),
            ..Default::default()
        }
    }

    #[test]
    fn offline_names_the_device_from_the_message() {
        let f = frame(
            Some("tool_offline"),
            Some("kai"),
            "MCP server 'kai': device 'Kai desktop' is offline (never seen)",
        );
        assert_eq!(
            said(&f),
            Said::Notice("Kai desktop is offline. Answered without its tools.".into())
        );
        let bare = frame(Some("tool_offline"), Some("kai"), "gone");
        assert_eq!(
            said(&bare),
            Said::Notice("kai is offline. Answered without its tools.".into())
        );
    }

    #[test]
    fn unreachable_says_how_to_grant() {
        let f = frame(Some("tool_unreachable"), Some("kai"), "anything");
        let Said::Notice(t) = said(&f) else { panic!() };
        assert!(t.starts_with("This phone may not use kai's tools."), "{t}");
        assert!(t.contains("Device tools"));
    }

    #[test]
    fn the_other_label_codes_show_the_message_plainly() {
        for c in ["tool_disabled", "tool_unknown", "tool_failed"] {
            assert_eq!(
                said(&frame(Some(c), Some("x"), "it broke")),
                Said::Notice("it broke".into())
            );
        }
    }

    #[test]
    fn unknown_and_absent_codes_are_the_turns_error() {
        assert_eq!(
            said(&frame(Some("gpu_hold"), None, "held")),
            Said::Error("held (gpu_hold)".into())
        );
        assert_eq!(said(&frame(None, None, "boom")), Said::Error("boom".into()));
    }
}
