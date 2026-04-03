// This is the root of the frame-ui crate.
// It will handle display drivers, touch input, image decoding, and the UI overlay.

pub mod adapter;
pub mod display;
pub mod input;

slint::include_modules!();

pub use adapter::sync_window_state;
pub use adapter::{UiAdapter, UiStateSnapshot, create_ui, ui_state_snapshot};
