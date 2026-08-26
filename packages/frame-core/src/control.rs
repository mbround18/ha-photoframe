use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionType {
    Cut,
    Fade,
    SlideLeft,
    SlideRight,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceCommand {
    Reboot,
    ReloadUi,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IncomingControlMessage {
    pub media_url: Option<String>,
    pub transition_type: Option<TransitionType>,
    pub brightness: Option<u8>,
    pub correlation_id: Option<String>,
    pub cmd: Option<DeviceCommand>,
    pub registration: Option<ControllerRegistration>,
    /// Defaults to showing the photo, so a controller too old to send this
    /// keeps behaving exactly as it did.
    #[serde(default)]
    pub queue: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControllerRegistration {
    pub claimed: bool,
    pub display_name: Option<String>,
    pub message: Option<String>,
    /// The token this frame presents when downloading photos.
    ///
    /// Minted by Home Assistant, never chosen by the frame, and the only
    /// credential the frame holds besides the Wi-Fi password (Principle II).
    /// Without it every photo request is refused, so a frame that has not been
    /// told its token can connect and still show nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_token: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RenderPresentation {
    pub transition_type: Option<TransitionType>,
    pub brightness: Option<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RenderRequest {
    pub media_url: String,
    pub correlation_id: Option<String>,
    pub presentation: RenderPresentation,
    /// Hold this one in reserve rather than showing it.
    ///
    /// The controller sends spares so a tap has something ready and never
    /// waits on a download. Without this every photo would be "show now" and
    /// a spare would silently replace the picture someone is looking at.
    pub queue: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandRequest {
    pub command: DeviceCommand,
    pub correlation_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlEvent {
    Render(RenderRequest),
    Command(CommandRequest),
    Registration(ControllerRegistration),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenStatus {
    Idle,
    Rendering,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeviceHealth {
    pub cpu_temp_millidegrees: Option<i32>,
    pub screen_status: Option<ScreenStatus>,
    /// Plain-language state of the SD cache, e.g. "ready (61035 MB)" or
    /// "no card detected".
    ///
    /// Reported rather than shown on the panel: an adopted frame shows photos
    /// and nothing else (Principle VIII). Without a card the frame still works,
    /// but only from its in-memory buffer, so this is a degradation the owner
    /// should be able to see in Home Assistant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Decoded photos ready to show, including the one on screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffered_photos: Option<u8>,
    /// Where the photos on screen are coming from.
    ///
    /// A frame running from the owner's own SD card photos deliberately ignores
    /// everything Home Assistant sends. Reporting that is what stops it looking
    /// like a frame that has stopped working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo_source: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum OutboundStatusMessage {
    Connected {
        device_id: String,
        device_name: String,
    },
    RenderStarted {
        media_url: String,
        correlation_id: Option<String>,
    },
    RenderCompleted {
        media_url: String,
        correlation_id: Option<String>,
    },
    CommandAcknowledged {
        cmd: DeviceCommand,
        correlation_id: Option<String>,
    },
    Health(DeviceHealth),
    /// The frame would like more photos than it currently holds.
    ///
    /// `wanted` is how many would fill its cache, so the controller can send a
    /// batch rather than trickle one photo per request. Advisory: the
    /// controller may send fewer or none, and the frame keeps showing what it
    /// has either way.
    PhotoRequested {
        wanted: u16,
        cached: u16,
    },
    Error {
        message: String,
        correlation_id: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlMessageError {
    InvalidJson(String),
    MissingAction,
    ConflictingAction,
    EmptyMediaUrl,
    MetadataWithoutMediaUrl,
    InvalidBrightness(u8),
}

impl fmt::Display for ControlMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(f, "invalid control message JSON: {error}"),
            Self::MissingAction => {
                write!(f, "control message must include either media_url or cmd")
            }
            Self::ConflictingAction => {
                write!(f, "control message cannot include both media_url and cmd")
            }
            Self::EmptyMediaUrl => write!(f, "control message media_url cannot be empty"),
            Self::MetadataWithoutMediaUrl => {
                write!(f, "control message metadata requires a media_url payload")
            }
            Self::InvalidBrightness(value) => write!(
                f,
                "control message brightness must be between 0 and 100 inclusive, got {value}"
            ),
        }
    }
}

impl Error for ControlMessageError {}

pub fn parse_control_message(payload: &str) -> Result<ControlEvent, ControlMessageError> {
    let message: IncomingControlMessage = serde_json::from_str(payload)
        .map_err(|error| ControlMessageError::InvalidJson(error.to_string()))?;
    validate_control_message(message)
}

pub fn validate_control_message(
    message: IncomingControlMessage,
) -> Result<ControlEvent, ControlMessageError> {
    if let Some(brightness) = message.brightness
        && brightness > 100
    {
        return Err(ControlMessageError::InvalidBrightness(brightness));
    }

    let action_count = usize::from(message.media_url.is_some())
        + usize::from(message.cmd.is_some())
        + usize::from(message.registration.is_some());

    if action_count > 1 {
        return Err(ControlMessageError::ConflictingAction);
    }

    match (message.media_url, message.cmd, message.registration) {
        (Some(media_url), None, None) => {
            let trimmed_media_url = media_url.trim();
            if trimmed_media_url.is_empty() {
                return Err(ControlMessageError::EmptyMediaUrl);
            }

            Ok(ControlEvent::Render(RenderRequest {
                media_url: trimmed_media_url.to_string(),
                correlation_id: message.correlation_id,
                queue: message.queue,
                presentation: RenderPresentation {
                    transition_type: message.transition_type,
                    brightness: message.brightness,
                },
            }))
        }
        (None, Some(command), None) => {
            if message.transition_type.is_some() || message.brightness.is_some() {
                return Err(ControlMessageError::MetadataWithoutMediaUrl);
            }

            Ok(ControlEvent::Command(CommandRequest {
                command,
                correlation_id: message.correlation_id,
            }))
        }
        (None, None, Some(registration)) => {
            if message.transition_type.is_some() || message.brightness.is_some() {
                return Err(ControlMessageError::MetadataWithoutMediaUrl);
            }

            Ok(ControlEvent::Registration(registration))
        }
        (None, None, None) => Err(ControlMessageError::MissingAction),
        _ => Err(ControlMessageError::ConflictingAction),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControlEvent, ControlMessageError, ControllerRegistration, DeviceCommand,
        IncomingControlMessage, TransitionType, parse_control_message, validate_control_message,
    };

    #[test]
    fn parses_render_message_with_optional_metadata() {
        let event = parse_control_message(
            r#"{
                "media_url": "https://example.com/photo.jpg",
                "transition_type": "fade",
                "brightness": 64,
                "correlation_id": "abc123"
            }"#,
        )
        .expect("render message should parse");

        assert_eq!(
            event,
            ControlEvent::Render(super::RenderRequest {
                media_url: "https://example.com/photo.jpg".to_string(),
                correlation_id: Some("abc123".to_string()),
                // A controller that says nothing means "show it", so an older
                // one keeps behaving exactly as it did.
                queue: false,
                presentation: super::RenderPresentation {
                    transition_type: Some(TransitionType::Fade),
                    brightness: Some(64),
                },
            })
        );
    }

    #[test]
    fn parses_command_message() {
        let event = parse_control_message(
            r#"{
                "cmd": "reload_ui",
                "correlation_id": "cmd-7"
            }"#,
        )
        .expect("command message should parse");

        assert_eq!(
            event,
            ControlEvent::Command(super::CommandRequest {
                command: DeviceCommand::ReloadUi,
                correlation_id: Some("cmd-7".to_string()),
            })
        );
    }

    #[test]
    fn rejects_metadata_without_media_url() {
        let error = validate_control_message(IncomingControlMessage {
            media_url: None,
            transition_type: Some(TransitionType::Cut),
            brightness: None,
            correlation_id: None,
            queue: false,
            cmd: Some(DeviceCommand::Reboot),
            registration: None,
        })
        .expect_err("command message should reject render metadata");

        assert_eq!(error, ControlMessageError::MetadataWithoutMediaUrl);
    }

    #[test]
    fn rejects_brightness_out_of_range() {
        let error = parse_control_message(
            r#"{
                "media_url": "https://example.com/photo.jpg",
                "brightness": 255
            }"#,
        )
        .expect_err("brightness > 100 must fail validation");

        assert_eq!(error, ControlMessageError::InvalidBrightness(255));
    }

    #[test]
    fn parses_registration_message() {
        let event = parse_control_message(
            r#"{
                "registration": {
                    "claimed": true,
                    "display_name": "Kitchen Frame",
                    "message": "Claimed in Home Assistant",
                    "frame_token": "tok-abc123"
                }
            }"#,
        )
        .expect("registration message should parse");

        assert_eq!(
            event,
            ControlEvent::Registration(ControllerRegistration {
                claimed: true,
                display_name: Some("Kitchen Frame".to_string()),
                message: Some("Claimed in Home Assistant".to_string()),
                frame_token: Some("tok-abc123".to_string()),
            })
        );
    }
}
