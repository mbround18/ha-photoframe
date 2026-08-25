// This is the main execution crate for the photo frame firmware.
// It imports all the other crates, wires up the hardware, and starts the
// application loops.

#[cfg(target_os = "espidf")]
use anyhow::Context;

#[cfg(target_os = "espidf")]
use esp_idf_svc::nvs::EspDefaultNvsPartition;

#[cfg(target_os = "espidf")]
mod runtime;

#[cfg(target_os = "espidf")]
mod setup_state_store;

#[cfg(target_os = "espidf")]
unsafe extern "C" {
    /// T056 spike: bring up a connectable BLE peripheral through the C6.
    /// See packages/frame-firmware/components/frame_ble_spike/.
    fn frame_ble_spike_start(device_name: *const core::ffi::c_char) -> i32;
}

#[cfg(target_os = "espidf")]
// The control channel is hosted on Home Assistant's own web server, so it
// shares port 8123 rather than opening a second listener.
const DEFAULT_CONTROL_PLANE_URL: &str = "ws://homeassistant.local:8123/api/photoframe_bridge/ws";

/// Where to look for the Home Assistant control plane.
///
/// Adoption will teach the frame this address (T067); until then a build-time
/// `HA_CONTROL_URL` in the workspace `.env` lets a development board be pointed
/// at an instance directly, which also covers networks where `.local` does not
/// resolve.
#[cfg(target_os = "espidf")]
fn control_plane_url() -> &'static str {
    match option_env!("HA_CONTROL_URL") {
        Some(url) if !url.is_empty() => url,
        _ => DEFAULT_CONTROL_PLANE_URL,
    }
}

#[cfg(target_os = "espidf")]
fn rom_print(message: &'static [u8]) {
    debug_assert_eq!(message.last().copied(), Some(0));

    unsafe {
        esp_idf_sys::ets_printf(message.as_ptr().cast());
    }
}

#[cfg(target_os = "espidf")]
fn init_tracing() -> anyhow::Result<()> {
    use tracing_log::LogTracer;
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;

    let _ = LogTracer::init();

    let fmt_layer = fmt::layer()
        .with_ansi(false)
        .without_time()
        .with_target(true)
        .with_line_number(true)
        .with_file(false)
        .compact();

    tracing_subscriber::registry()
        .with(fmt_layer)
        .try_init()
        .map_err(|err| anyhow::anyhow!("failed to initialize tracing subscriber: {err}"))?;

    Ok(())
}

#[cfg(target_os = "espidf")]
fn quiet_hosted_component_logs() {
    unsafe {
        esp_idf_sys::esp_log_level_set(
            b"co-pro-main\0".as_ptr().cast(),
            esp_idf_sys::esp_log_level_t_ESP_LOG_WARN,
        );
        esp_idf_sys::esp_log_level_set(
            b"rpc_rsp\0".as_ptr().cast(),
            esp_idf_sys::esp_log_level_t_ESP_LOG_WARN,
        );
    }
}

#[cfg(target_os = "espidf")]
fn device_identity() -> anyhow::Result<(String, String)> {
    let mut mac = [0_u8; 6];
    let result = unsafe { esp_idf_sys::esp_efuse_mac_get_default(mac.as_mut_ptr()) };
    if result != esp_idf_sys::ESP_OK {
        anyhow::bail!("failed to read base MAC address for device identity: {result}");
    }

    let device_id = format!(
        "esp32p4-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    let device_name = format!("Photo Frame {:02X}{:02X}", mac[4], mac[5]);
    Ok((device_id, device_name))
}

#[cfg(target_os = "espidf")]
fn main() {
    rom_print(b"frame-firmware: entered Rust main\r\n\0");

    // This is the entry point of our application.
    //
    // For the first ESP32-P4 smoke test we keep startup minimal and only link
    // the required ESP-IDF patches before composing the current Rust layers.
    esp_idf_sys::link_patches();

    const APP_THREAD_STACK_SIZE: usize = 128 * 1024;

    let app_thread = std::thread::Builder::new()
        .name("frame-app".to_string())
        .stack_size(APP_THREAD_STACK_SIZE)
        .spawn(run)
        .expect("failed to spawn frame-app thread");

    let run_result = match app_thread.join() {
        Ok(result) => result,
        Err(_) => Err(anyhow::anyhow!("frame-app thread panicked")),
    };

    if let Err(err) = run_result {
        rom_print(b"frame-firmware: run() returned error\r\n\0");
        eprintln!("fatal firmware startup error: {err:#}");

        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

#[cfg(target_os = "espidf")]
fn persist_setup_checkpoint(
    store: &setup_state_store::SetupStateStore,
    checkpoint: setup_state_store::SetupCheckpoint,
    app_state: &frame_core::AppState,
) {
    match store.save_checkpoint(checkpoint, app_state) {
        Ok(snapshot) => {
            tracing::info!(
                target: "frame_firmware",
                checkpoint = snapshot.checkpoint.as_str(),
                phase = snapshot.app_phase.as_str(),
                network = snapshot.network_phase.as_str(),
                flags = snapshot.flags_summary(),
                "persisted setup checkpoint"
            );
        }
        Err(error) => {
            tracing::warn!(
                target: "frame_firmware",
                checkpoint = checkpoint.as_str(),
                "failed to persist setup checkpoint: {error}"
            );
        }
    }
}

#[cfg(target_os = "espidf")]
fn run() -> anyhow::Result<()> {
    rom_print(b"frame-firmware: starting run() initialization\r\n\0");
    init_tracing()?;
    rom_print(b"frame-firmware: tracing initialized\r\n\0");

    tracing::info!(target: "frame_firmware", "starting up photo frame firmware");

    let peripherals = esp_idf_svc::hal::peripherals::Peripherals::take()?;
    let sys_loop = esp_idf_svc::eventloop::EspSystemEventLoop::take()?;

    frame_net::wifi::init_wifi_manager(peripherals.modem, sys_loop.clone())?;
    quiet_hosted_component_logs();

    let mut app_state = frame_core::AppState::new();
    let (device_id, device_name) = device_identity()?;
    app_state.set_device_identity(device_id, device_name);
    let nvs_partition =
        EspDefaultNvsPartition::take().context("failed to open default NVS partition")?;
    let setup_state_store = setup_state_store::SetupStateStore::new(nvs_partition)?;
    let mut provisioning_manager = frame_net::create_provisioning_manager();
    let mut ui = frame_ui::create_ui()?;
    rom_print(b"frame-firmware: UI adapter created\r\n\0");

    match setup_state_store.load() {
        Ok(Some(previous_state)) => {
            tracing::info!(
                target: "frame_firmware",
                checkpoint = previous_state.checkpoint.as_str(),
                phase = previous_state.app_phase.as_str(),
                network = previous_state.network_phase.as_str(),
                flags = previous_state.flags_summary(),
                "loaded previous persisted setup checkpoint"
            );
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                target: "frame_firmware",
                "failed to load previous setup checkpoint: {error}"
            );
        }
    }

    persist_setup_checkpoint(
        &setup_state_store,
        setup_state_store::SetupCheckpoint::BootStarted,
        &app_state,
    );

    {
        let _span = tracing::info_span!(
            "firmware_boot",
            phase = app_state.phase.as_str(),
            network = app_state.network_phase.as_str()
        )
        .entered();
        ui.sync(&app_state)?;
        tracing::info!(
            target: "frame_firmware",
            phase = app_state.phase.as_str(),
            network = app_state.network_phase.as_str(),
            "initial splash rendered"
        );
    }
    persist_setup_checkpoint(
        &setup_state_store,
        setup_state_store::SetupCheckpoint::SplashRendered,
        &app_state,
    );

    std::thread::sleep(std::time::Duration::from_millis(1500));

    app_state.begin_setup();
    ui.sync(&app_state)?;
    tracing::info!(
        target: "frame_firmware",
        phase = app_state.phase.as_str(),
        network = app_state.network_phase.as_str(),
        "setup screen rendered"
    );
    persist_setup_checkpoint(
        &setup_state_store,
        setup_state_store::SetupCheckpoint::SetupRendered,
        &app_state,
    );

    loop {
        let network_phase = {
            let _span = tracing::info_span!("network_provisioning").entered();
            provisioning_manager.ensure_network()?
        };
        app_state.set_network_phase(network_phase.clone());
        ui.sync(&app_state)?;
        tracing::info!(
            target: "frame_firmware",
            network = app_state.network_phase.as_str(),
            "network phase updated"
        );
        persist_setup_checkpoint(
            &setup_state_store,
            match app_state.network_phase {
                frame_core::NetworkPhase::Provisioning => {
                    setup_state_store::SetupCheckpoint::Provisioning
                }
                frame_core::NetworkPhase::Connected => {
                    setup_state_store::SetupCheckpoint::NetworkConnected
                }
                frame_core::NetworkPhase::Unprovisioned => {
                    setup_state_store::SetupCheckpoint::SetupRendered
                }
            },
            &app_state,
        );

        if app_state.network_phase == frame_core::NetworkPhase::Provisioning {
            if let Some((ssid, password)) = provisioning_manager.get_provisioning_ap_details() {
                app_state.set_provisioning_details(ssid, password);
            }
            ui.sync(&app_state)?;
        }

        if app_state.network_phase == frame_core::NetworkPhase::Connected {
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Network is up. From here the frame's job is to be discovered by Home
    // Assistant, get adopted, and then run a slideshow from its SD cache.
    //
    // The on-device Google device-code OAuth flow and the LAN album-selection
    // portal that used to live here were removed: Home Assistant now owns all
    // photo sourcing and every third-party credential (Constitution Principle
    // II, FR-008, FR-043).
    //
    // Still to be built, in dependency order:
    //   - T063  mDNS announcement of _photoframe._tcp.local.
    //   - T058  Improv Wi-Fi over BLE for first-run provisioning
    //   - T030  WebSocket control client + claim handshake (T072)
    //   - T038  SD card cache mount
    //   - T040  cache-first slideshow
    // See specs/001-ha-managed-photo-frame/tasks.md.

    persist_setup_checkpoint(
        &setup_state_store,
        setup_state_store::SetupCheckpoint::NetworkConnected,
        &app_state,
    );

    // Connect to the Home Assistant control plane. The runtime owns its own
    // reconnect loop, so a controller that is down, restarting, or not yet
    // configured is not an error here -- the frame simply keeps retrying while
    // continuing to show whatever it already has (FR-026).
    let controller_url = control_plane_url();
    let thin_client_runtime = runtime::ThinClientRuntime::spawn(
        runtime::WebSocketControlPlaneTransport::new(controller_url),
        runtime::MediaRenderExecutor,
    );

    app_state.set_controller_phase(frame_core::ControllerPhase::Searching);
    frame_ui::set_controller_phase(app_state.controller_phase.clone())?;
    ui.sync(&app_state)?;

    if let (Some(device_id), Some(device_name)) =
        (app_state.device_id.clone(), app_state.device_name.clone())
    {
        // Announce ourselves so the controller can associate this connection
        // with a frame. The full claim handshake lands in T072.
        if let Err(error) =
            thin_client_runtime.send_status(frame_core::OutboundStatusMessage::Connected {
                device_id,
                device_name,
            })
        {
            tracing::warn!(target: "frame_firmware", "could not announce to controller: {error}");
        }
    }

    tracing::info!(
        target: "frame_firmware",
        device_id = app_state.device_id.as_deref().unwrap_or_default(),
        controller_url,
        "control-plane runtime started"
    );

    // T056 spike: prove a connectable BLE peripheral works on this board before
    // committing to Improv-over-BLE provisioning (research.md R3/R9). A failure
    // here is not fatal -- the spike is diagnostic, and the SoftAP fallback
    // exists precisely for this outcome.
    {
        let advert_name = app_state
            .device_id
            .as_deref()
            .and_then(|id| id.rsplit('-').next())
            .map(|suffix| {
                let tail = if suffix.len() >= 4 {
                    &suffix[suffix.len() - 4..]
                } else {
                    suffix
                };
                format!("PhotoFrame-{}", tail.to_uppercase())
            })
            .unwrap_or_else(|| "PhotoFrame".to_string());

        match std::ffi::CString::new(advert_name.clone()) {
            Ok(name) => {
                let rc = unsafe { frame_ble_spike_start(name.as_ptr()) };
                if rc == 0 {
                    tracing::info!(
                        target: "frame_firmware",
                        advert_name = advert_name.as_str(),
                        "T056 spike: BLE peripheral started"
                    );
                } else {
                    tracing::error!(
                        target: "frame_firmware",
                        rc,
                        "T056 spike: BLE peripheral failed to start"
                    );
                }
            }
            Err(error) => {
                tracing::error!(target: "frame_firmware", "invalid BLE name: {error}");
            }
        }
    }

    rom_print(b"frame-firmware: entering UI run loop\r\n\0");

    // The panel only changes when the state behind it changes, so this loop is
    // a cheap reconcile rather than a render loop. Photos arrive on the
    // control-plane thread and land here through the rendered-image store.
    loop {
        ui.sync(&app_state)?;
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    tracing::warn!(
        "frame-firmware only runs on the espidf target; use `cargo firmware-check` or `cargo firmware-build` from the workspace root."
    );
}
