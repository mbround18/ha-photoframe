// This is the root of the frame-core crate.
// It will contain the core business logic, state machines, and shared data models.

pub mod control;
pub mod models;
pub mod state;

pub use control::{
    CommandRequest, ControlEvent, ControlMessageError, ControllerRegistration, DeviceCommand,
    DeviceHealth, IncomingControlMessage, OutboundStatusMessage, RenderPresentation, RenderRequest,
    ScreenStatus, TransitionType, parse_control_message,
};
pub use state::{AppPhase, AppState, ControllerPhase, NetworkPhase};
