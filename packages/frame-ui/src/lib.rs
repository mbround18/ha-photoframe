// This is the root of the frame-ui crate.
// It will handle display drivers, touch input, image decoding, and the UI overlay.

pub mod adapter;
pub mod controller_state;
pub mod display;
pub mod input;
pub mod rendered_image;

slint::include_modules!();

pub use adapter::sync_window_state;
pub use adapter::{UiAdapter, UiStateSnapshot, create_ui, ui_state_snapshot};
pub use controller_state::{ControllerStateSnapshot, controller_state_snapshot, set_controller_phase};
pub use rendered_image::{
	RenderedImage, RenderedImageSnapshot, clear_rendered_image, rendered_image_snapshot,
	set_rendered_image,
};
