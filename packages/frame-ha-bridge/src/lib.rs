use frame_core::{
    CommandRequest, ControlEvent, DeviceCommand, IncomingControlMessage, TransitionType,
    parse_control_message,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass(get_all)]
struct NativeParsedControlMessage {
    kind: String,
    media_url: Option<String>,
    command: Option<String>,
    transition_type: Option<String>,
    brightness: Option<u8>,
    correlation_id: Option<String>,
}

#[pyfunction]
fn parse_control_payload(payload: &str) -> PyResult<NativeParsedControlMessage> {
    parse_control_message(payload)
        .map(NativeParsedControlMessage::from)
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

#[pyfunction]
#[pyo3(signature = (media_url, transition_type=None, brightness=None, correlation_id=None))]
fn build_render_payload(
    media_url: &str,
    transition_type: Option<&str>,
    brightness: Option<u8>,
    correlation_id: Option<&str>,
) -> PyResult<String> {
    let incoming = IncomingControlMessage {
        media_url: Some(media_url.to_string()),
        transition_type: transition_type
            .map(parse_transition_type)
            .transpose()
            .map_err(PyValueError::new_err)?,
        brightness,
        correlation_id: correlation_id.map(ToOwned::to_owned),
        cmd: None,
        registration: None,
    };

    serde_json::to_string(&incoming).map_err(|error| PyValueError::new_err(error.to_string()))
}

#[pyfunction]
#[pyo3(signature = (command, correlation_id=None))]
fn build_command_payload(command: &str, correlation_id: Option<&str>) -> PyResult<String> {
    let request = CommandRequest {
        command: parse_device_command(command).map_err(PyValueError::new_err)?,
        correlation_id: correlation_id.map(ToOwned::to_owned),
    };

    let incoming = IncomingControlMessage {
        media_url: None,
        transition_type: None,
        brightness: None,
        correlation_id: request.correlation_id,
        cmd: Some(request.command),
        registration: None,
    };

    serde_json::to_string(&incoming).map_err(|error| PyValueError::new_err(error.to_string()))
}

#[pymodule]
fn _native(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeParsedControlMessage>()?;
    module.add_function(wrap_pyfunction!(parse_control_payload, module)?)?;
    module.add_function(wrap_pyfunction!(build_render_payload, module)?)?;
    module.add_function(wrap_pyfunction!(build_command_payload, module)?)?;
    Ok(())
}

impl From<ControlEvent> for NativeParsedControlMessage {
    fn from(event: ControlEvent) -> Self {
        match event {
            ControlEvent::Render(request) => Self {
                kind: "render".to_string(),
                media_url: Some(request.media_url),
                command: None,
                transition_type: request.presentation.transition_type.map(transition_name),
                brightness: request.presentation.brightness,
                correlation_id: request.correlation_id,
            },
            ControlEvent::Command(request) => Self {
                kind: "command".to_string(),
                media_url: None,
                command: Some(command_name(request.command).to_string()),
                transition_type: None,
                brightness: None,
                correlation_id: request.correlation_id,
            },
            ControlEvent::Registration(_registration) => Self {
                kind: "registration".to_string(),
                media_url: None,
                command: None,
                transition_type: None,
                brightness: None,
                correlation_id: None,
            },
        }
    }
}

fn parse_transition_type(value: &str) -> Result<TransitionType, String> {
    match value {
        "cut" => Ok(TransitionType::Cut),
        "fade" => Ok(TransitionType::Fade),
        "slide_left" => Ok(TransitionType::SlideLeft),
        "slide_right" => Ok(TransitionType::SlideRight),
        _ => Err(format!("unsupported transition_type: {value}")),
    }
}

fn parse_device_command(value: &str) -> Result<DeviceCommand, String> {
    match value {
        "reboot" => Ok(DeviceCommand::Reboot),
        "reload_ui" => Ok(DeviceCommand::ReloadUi),
        _ => Err(format!("unsupported device command: {value}")),
    }
}

fn transition_name(transition_type: TransitionType) -> String {
    match transition_type {
        TransitionType::Cut => "cut".to_string(),
        TransitionType::Fade => "fade".to_string(),
        TransitionType::SlideLeft => "slide_left".to_string(),
        TransitionType::SlideRight => "slide_right".to_string(),
    }
}

fn command_name(command: DeviceCommand) -> &'static str {
    match command {
        DeviceCommand::Reboot => "reboot",
        DeviceCommand::ReloadUi => "reload_ui",
    }
}

#[cfg(test)]
mod tests {
    use super::{build_command_payload, build_render_payload, parse_control_payload};

    #[test]
    fn builds_render_payload() {
        let payload = build_render_payload(
            "https://example.com/photo.jpg",
            Some("fade"),
            Some(55),
            Some("abc-1"),
        )
        .expect("render payload should build");

        let parsed = parse_control_payload(&payload).expect("built payload should parse");
        assert_eq!(parsed.kind, "render");
        assert_eq!(parsed.media_url.as_deref(), Some("https://example.com/photo.jpg"));
        assert_eq!(parsed.transition_type.as_deref(), Some("fade"));
        assert_eq!(parsed.brightness, Some(55));
    }

    #[test]
    fn builds_command_payload() {
        let payload = build_command_payload("reload_ui", Some("cmd-2"))
            .expect("command payload should build");

        let parsed = parse_control_payload(&payload).expect("built payload should parse");
        assert_eq!(parsed.kind, "command");
        assert_eq!(parsed.command.as_deref(), Some("reload_ui"));
        assert_eq!(parsed.correlation_id.as_deref(), Some("cmd-2"));
    }

    #[test]
    fn rejects_unknown_command() {
        let error = build_command_payload("unsupported", None)
            .expect_err("unsupported commands must fail");

        assert!(error.to_string().contains("unsupported device command"));
    }
}