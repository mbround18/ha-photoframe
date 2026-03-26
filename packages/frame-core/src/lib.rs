// This is the root of the frame-core crate.
// It will contain the core business logic, state machines, and shared data models.

pub mod models;
pub mod state;

pub use state::{AppPhase, AppState, NetworkPhase};
