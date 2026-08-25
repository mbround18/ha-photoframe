//! The frame's display surface.
//!
//! Drawing is done with embedded-graphics straight onto the panel, following
//! the same model as ha-kiosk. There is no UI toolkit: the frame shows a
//! handful of setup strings and then photos, and a retained-mode toolkit costs
//! more flash than the rest of the firmware while buying nothing here.

pub mod controller_state;
pub mod rendered_image;

// Touch is deliberately absent: this board's GSL3680 controller ships without
// a licence (see components/vendor), and nothing in the product needs touch --
// the factory-reset gesture uses the BOOT button on GPIO35 instead.

#[cfg(target_os = "espidf")]
pub mod panel;

#[cfg(target_os = "espidf")]
pub mod screens;

#[cfg(target_os = "espidf")]
pub mod ui;

pub use controller_state::{
    ControllerStateSnapshot, controller_state_snapshot, set_controller_phase,
};
pub use rendered_image::{
    RenderedImage, RenderedImageSnapshot, clear_rendered_image, rendered_image_snapshot,
    set_rendered_image,
};

#[cfg(target_os = "espidf")]
pub use ui::{FrameUi, create_ui};
