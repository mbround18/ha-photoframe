use anyhow::Result;
use frame_core::{AppState, ControllerPhase};
use slint::ComponentHandle;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::rc::Rc;

#[cfg(target_os = "espidf")]
use std::cell::Cell;

use anyhow::anyhow;

#[cfg(target_os = "espidf")]
use crate::controller_state::controller_state_snapshot;
use crate::rendered_image::{RenderedImage, rendered_image_snapshot};

#[cfg(target_os = "espidf")]
use slint::PhysicalSize;

#[cfg(target_os = "espidf")]
use esp_idf_hal::task::do_yield;

#[cfg(target_os = "espidf")]
use slint::platform::software_renderer::{
    LineBufferProvider, MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel,
};

#[cfg(target_os = "espidf")]
use slint::platform::{Platform, PlatformError, WindowAdapter};

#[cfg(target_os = "espidf")]
use std::sync::OnceLock;

#[cfg(target_os = "espidf")]
use std::time::Instant;

use crate::MainWindow;

#[cfg(target_os = "espidf")]
thread_local! {
    static SLINT_SOFTWARE_WINDOW: Rc<MinimalSoftwareWindow> =
        MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
}

#[cfg(target_os = "espidf")]
static SLINT_PLATFORM_STATUS: OnceLock<Result<(), String>> = OnceLock::new();

#[cfg(target_os = "espidf")]
static SLINT_PLATFORM_START: OnceLock<Instant> = OnceLock::new();

#[cfg(target_os = "espidf")]
fn embedded_watchdog_tick() {
    unsafe {
        if esp_idf_sys::esp_task_wdt_status(core::ptr::null_mut()) == esp_idf_sys::ESP_OK {
            let _ = esp_idf_sys::esp_task_wdt_reset();
        }
    }
    do_yield();
}

#[cfg(target_os = "espidf")]
struct WatchdogFrameBuffer<'a> {
    frame_buffer: &'a mut [Rgb565Pixel],
    stride: usize,
    lines_since_tick: usize,
}

#[cfg(target_os = "espidf")]
impl LineBufferProvider for WatchdogFrameBuffer<'_> {
    type TargetPixel = Rgb565Pixel;

    fn process_line(
        &mut self,
        line: usize,
        range: core::ops::Range<usize>,
        render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    ) {
        let line_begin = line * self.stride;
        render_fn(&mut self.frame_buffer[line_begin..][range]);

        self.lines_since_tick += 1;
        if self.lines_since_tick >= 16 {
            embedded_watchdog_tick();
            self.lines_since_tick = 0;
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiStateSnapshot {
    pub app_phase: String,
    pub headline_text: String,
    pub status_text: String,
    pub network_status: String,
    pub controller_status: String,
    pub detail_text: String,
    pub provisioning_ssid: String,
    pub provisioning_password: String,
}

impl UiStateSnapshot {
    pub fn from_app_state(state: &AppState) -> Self {
        Self {
            app_phase: app_phase_text(state).to_string(),
            headline_text: headline_text(state).to_string(),
            status_text: status_text(state).to_string(),
            network_status: state.network_phase.as_str().to_string(),
            controller_status: state.controller_phase.as_str().to_string(),
            detail_text: detail_text(state),
            provisioning_ssid: state
                .provisioning_ssid
                .as_deref()
                .unwrap_or_default()
                .to_string(),
            provisioning_password: state
                .provisioning_password
                .as_deref()
                .unwrap_or_default()
                .to_string(),
        }
    }
}

pub trait UiAdapter {
    fn sync_state(&mut self, state: &AppState) -> Result<()>;
    fn run(&self) -> Result<()>;
}

pub fn create_ui() -> Result<Box<dyn UiAdapter>> {
    #[cfg(target_os = "espidf")]
    {
        return Ok(Box::new(SlintFirmwareUiAdapter::new()?));
    }

    #[cfg(not(target_os = "espidf"))]
    {
        Ok(Box::new(SlintUiAdapter::new()?))
    }
}

pub fn ui_state_snapshot(state: &AppState) -> UiStateSnapshot {
    UiStateSnapshot::from_app_state(state)
}

#[cfg(target_os = "espidf")]
struct EmbeddedSlintPlatform;

#[cfg(target_os = "espidf")]
impl Platform for EmbeddedSlintPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(SLINT_SOFTWARE_WINDOW.with(|window| window.clone()))
    }

    fn duration_since_start(&self) -> core::time::Duration {
        let start = SLINT_PLATFORM_START.get_or_init(Instant::now);
        Instant::now().saturating_duration_since(*start)
    }
}

#[cfg(target_os = "espidf")]
fn ensure_embedded_slint_platform() -> Result<()> {
    SLINT_PLATFORM_STATUS
        .get_or_init(|| {
            slint::platform::set_platform(Box::new(EmbeddedSlintPlatform))
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| anyhow!("failed to install embedded Slint platform: {error}"))
        .map(|_| ())
}

fn app_phase_text(state: &AppState) -> &'static str {
    state.phase.as_str()
}

fn headline_text(state: &AppState) -> &'static str {
    match state.phase {
        frame_core::AppPhase::Splash => "Welcome home",
        frame_core::AppPhase::Setup => "Let\'s get your frame ready",
        frame_core::AppPhase::Ready => match state.controller_phase {
            ControllerPhase::Connected => "Your frame is ready",
            _ => "Home Assistant setup continues here",
        },
    }
}

fn status_text(state: &AppState) -> &'static str {
    match state.phase {
        frame_core::AppPhase::Splash => "Booting up the display and preparing setup.",
        frame_core::AppPhase::Setup => "A few quick steps and your photos will start showing here.",
        frame_core::AppPhase::Ready => match state.controller_phase {
            ControllerPhase::Connected => {
                "Connected to Home Assistant and ready for incoming photos."
            }
            ControllerPhase::AwaitingConfiguration => {
                "Home Assistant found. Finish setup from Home Assistant to start sending photos."
            }
            ControllerPhase::Searching => "Looking for Home Assistant on your network.",
            ControllerPhase::Error(_) => "Home Assistant connection needs attention.",
            ControllerPhase::NotStarted => "Preparing the Home Assistant connection.",
        },
    }
}

fn detail_text(state: &AppState) -> String {
    match (state.phase.clone(), state.network_phase.clone()) {
        (frame_core::AppPhase::Splash, _) => {
            "We\'re starting services, checking connectivity, and getting your frame ready for first-time setup."
                .to_string()
        }
        (frame_core::AppPhase::Setup, frame_core::NetworkPhase::Provisioning) => {
            "Join the frame\'s temporary Wi-Fi network from your phone or laptop so we can connect it to your home internet."
                .to_string()
        }
        (frame_core::AppPhase::Setup, frame_core::NetworkPhase::Unprovisioned) => {
            "The frame is waiting to start setup. Network details and the next action will appear here shortly."
                .to_string()
        }
        (frame_core::AppPhase::Setup, frame_core::NetworkPhase::Connected) => {
            "Wi-Fi is connected. Finishing setup so Home Assistant can find this frame."
                .to_string()
        }
        (frame_core::AppPhase::Ready, _) => {
            let prefix = String::new();

            match &state.controller_phase {
                ControllerPhase::NotStarted => {
                    format!("{prefix}Preparing the Home Assistant control plane.")
                }
                ControllerPhase::Searching => {
                    format!("{prefix}Looking for your Home Assistant instance on the local network.")
                }
                ControllerPhase::AwaitingConfiguration => {
                    format!("{prefix}Home Assistant is reachable. Add or configure this frame in Home Assistant to begin sending photos.")
                }
                ControllerPhase::Connected => {
                    format!("{prefix}Home Assistant is connected and this frame is ready to receive photos.")
                }
                ControllerPhase::Error(error) => {
                    format!("{prefix}Home Assistant connection error: {error}")
                }
            }
        }
    }
}

pub fn sync_window_state(window: &MainWindow, state: &AppState) {
    apply_snapshot_to_window(window, &ui_state_snapshot(state));
}

fn apply_snapshot_to_window(window: &MainWindow, snapshot: &UiStateSnapshot) {
    window.set_app_phase(snapshot.app_phase.as_str().into());
    window.set_headline_text(snapshot.headline_text.as_str().into());
    window.set_status_text(snapshot.status_text.as_str().into());
    window.set_network_status(snapshot.network_status.as_str().into());
    window.set_controller_status(snapshot.controller_status.as_str().into());
    window.set_detail_text(snapshot.detail_text.as_str().into());
    window.set_provisioning_ssid(snapshot.provisioning_ssid.as_str().into());
    window.set_provisioning_password(snapshot.provisioning_password.as_str().into());

    let photos = current_photo_images();
    let photos_model = Rc::new(VecModel::from(photos));
    window.set_photos(photos_model.into());
}

/// The frame's current image comes from the rendered-image store, not from
/// the UI snapshot: the snapshot no longer carries provider photo metadata now
/// that Home Assistant owns photo sourcing.
fn current_photo_images() -> Vec<Image> {
    match rendered_image_snapshot() {
        Ok(rendered) => rendered
            .image
            .as_ref()
            .map(rendered_image_to_slint_image)
            .into_iter()
            .collect(),
        Err(error) => {
            log::warn!("failed to read rendered image state: {error:#}");
            Vec::new()
        }
    }
}

fn rendered_image_to_slint_image(image: &RenderedImage) -> Image {
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(image.width, image.height);
    let pixels = buffer.make_mut_slice();

    for (index, rgba) in image.rgba8.chunks_exact(4).enumerate() {
        pixels[index] = Rgba8Pixel {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
            a: rgba[3],
        };
    }

    Image::from_rgba8(buffer)
}

struct SlintUiAdapter {
    window: MainWindow,
}

impl SlintUiAdapter {
    fn new() -> Result<Self> {
        let window =
            MainWindow::new().map_err(|error| anyhow!("failed to create Slint window: {error}"))?;
        Ok(Self { window })
    }
}

impl UiAdapter for SlintUiAdapter {
    fn sync_state(&mut self, state: &AppState) -> Result<()> {
        sync_window_state(&self.window, state);

        Ok(())
    }

    fn run(&self) -> Result<()> {
        self.window
            .run()
            .map_err(|error| anyhow!("failed to run Slint window: {error}"))?;
        Ok(())
    }
}

#[cfg(target_os = "espidf")]
struct SlintFirmwareUiAdapter {
    window: MainWindow,
    software_window: Rc<MinimalSoftwareWindow>,
    display: crate::display::EmbeddedDisplay,
    input: crate::input::EmbeddedInput,
    controller_generation: Cell<u64>,
    rendered_generation: Cell<u64>,
}

#[cfg(target_os = "espidf")]
impl SlintFirmwareUiAdapter {
    fn new() -> Result<Self> {
        ensure_embedded_slint_platform()?;
        embedded_watchdog_tick();

        let display = crate::display::initialize_embedded_display()?;
        embedded_watchdog_tick();
        let input = crate::input::initialize_embedded_input(display.config())?;
        embedded_watchdog_tick();
        let software_window = SLINT_SOFTWARE_WINDOW.with(|window| window.clone());
        let size = PhysicalSize::new(
            u32::from(display.config().width),
            u32::from(display.config().height),
        );
        software_window.set_size(size);
        embedded_watchdog_tick();

        let window = MainWindow::new()
            .map_err(|error| anyhow!("failed to create embedded Slint window: {error}"))?;
        embedded_watchdog_tick();
        window
            .show()
            .map_err(|error| anyhow!("failed to show embedded Slint window: {error}"))?;
        embedded_watchdog_tick();

        log::info!(
            "embedded Slint window created for {}x{} @ {} degrees (touch: {})",
            display.config().width,
            display.config().height,
            display.config().rotation_degrees,
            input.is_touch_enabled()
        );

        Ok(Self {
            window,
            software_window,
            display,
            input,
            controller_generation: Cell::new(0),
            rendered_generation: Cell::new(0),
        })
    }

    fn apply_controller_phase_update_if_needed(&self) -> Result<()> {
        let controller = controller_state_snapshot()?;
        if controller.generation == self.controller_generation.get() {
            return Ok(());
        }

        self.controller_generation.set(controller.generation);
        self.window
            .set_controller_status(controller.phase.as_str().into());
        self.software_window.request_redraw();
        Ok(())
    }

    fn apply_rendered_image_update_if_needed(&self) -> Result<()> {
        let rendered = rendered_image_snapshot()?;
        if rendered.generation == self.rendered_generation.get() {
            return Ok(());
        }

        self.rendered_generation.set(rendered.generation);

        let photos = rendered
            .image
            .as_ref()
            .map(rendered_image_to_slint_image)
            .into_iter()
            .collect::<Vec<_>>();
        let photos_model = Rc::new(VecModel::from(photos));
        self.window.set_photos(photos_model.into());
        self.software_window.request_redraw();
        Ok(())
    }

    fn pump_frame(&self) -> Result<bool> {
        self.apply_controller_phase_update_if_needed()?;
        self.apply_rendered_image_update_if_needed()?;
        self.input.pump_window_events(&self.software_window)?;
        slint::platform::update_timers_and_animations();
        embedded_watchdog_tick();

        let stride = usize::from(self.display.config().width);
        let redrawn = self.software_window.draw_if_needed(|renderer| {
            let mut framebuffer = self.display.framebuffer();
            renderer.render_by_line(WatchdogFrameBuffer {
                frame_buffer: framebuffer.as_mut_slice(),
                stride,
                lines_since_tick: 0,
            });
        });

        if redrawn {
            embedded_watchdog_tick();
            self.display.present()?;
            embedded_watchdog_tick();
        }

        Ok(redrawn)
    }
}

#[cfg(target_os = "espidf")]
impl UiAdapter for SlintFirmwareUiAdapter {
    fn sync_state(&mut self, state: &AppState) -> Result<()> {
        apply_snapshot_to_window(&self.window, &ui_state_snapshot(state));
        self.software_window.request_redraw();
        let _ = self.pump_frame()?;
        Ok(())
    }

    fn run(&self) -> Result<()> {
        loop {
            let redrawn = self.pump_frame()?;
            let has_animations = self.software_window.has_active_animations();
            let sleep_for = if has_animations || redrawn {
                std::time::Duration::from_millis(16)
            } else {
                slint::platform::duration_until_next_timer_update()
                    .unwrap_or(std::time::Duration::from_millis(50))
            };

            std::thread::sleep(sleep_for.min(std::time::Duration::from_millis(50)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UiStateSnapshot;
    use frame_core::{AppState, NetworkPhase};

    #[test]
    fn snapshot_preserves_expected_state_fields() {
        let mut state = AppState::new();
        state.begin_setup();
        state.set_network_phase(NetworkPhase::Connected);
        state.set_device_identity("p4-a1b2c3d4e5f6", "Living Room Frame");

        let snapshot = UiStateSnapshot::from_app_state(&state);

        assert_eq!(snapshot.app_phase, "Setup");
        assert_eq!(snapshot.network_status, "Connected");
        assert!(snapshot.detail_text.contains("Wi-Fi is connected"));
    }
}
