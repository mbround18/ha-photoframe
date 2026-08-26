//! The frame's display surface.
//!
//! Drawing is done with embedded-graphics straight onto the panel, following
//! the same model as ha-kiosk. There is no UI toolkit: the frame shows a
//! handful of setup strings and then photos, and a retained-mode toolkit costs
//! more flash than the rest of the firmware while buying nothing here.

/// The panel's logical geometry, in the orientation photos are composed in.
///
/// The physical panel is 800x1280 portrait; the frame hangs landscape, so
/// everything upstream of the final rotation works in these terms. Canonical
/// here rather than in `panel` because `panel` is firmware-only and the
/// fitting logic has to be testable on a host.
pub const PANEL_LOGICAL_WIDTH: usize = 1280;
pub const PANEL_LOGICAL_HEIGHT: usize = 800;

pub mod controller_state;
pub mod fit;
pub mod local_photos;
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
pub use fit::{Treatment, fit_to_panel};
pub use local_photos::{LocalLibrary, scan_local_photos};
pub use rendered_image::{
    BUFFER_CAPACITY, RenderedImage, RenderedImageSnapshot, advance_rendered_image,
    clear_rendered_image, push_rendered_image, rendered_image_snapshot, set_rendered_image,
    show_rendered_image,
};

#[cfg(target_os = "espidf")]
pub use ui::{FrameUi, create_ui};
