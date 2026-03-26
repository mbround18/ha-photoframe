// This is the root of the frame-ui crate.
// It will handle display drivers, touch input, image decoding, and the UI overlay.

pub mod adapter;
pub mod display;
pub mod input;

#[cfg(not(target_os = "espidf"))]
slint::include_modules!();

pub use adapter::{UiAdapter, create_ui};
