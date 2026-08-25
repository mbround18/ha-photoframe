// This is the root of the frame-core crate.
// It will contain the core business logic, state machines, and shared data models.

pub mod control;
pub mod models;
pub mod state;

pub use control::{
	parse_control_message, CommandRequest, ControlEvent, ControlMessageError,
	ControllerRegistration, DeviceCommand, DeviceHealth, IncomingControlMessage, OutboundStatusMessage,
	RenderPresentation, RenderRequest, ScreenStatus, TransitionType,
};
pub use state::{AppPhase, AppState, ControllerPhase, NetworkPhase};
