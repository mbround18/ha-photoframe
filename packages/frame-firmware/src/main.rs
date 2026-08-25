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
const DEFAULT_CONTROL_PLANE_URL: &str = "ws://homeassistant.local:8765/ws";

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
        ui.sync_state(&app_state)?;
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
    ui.sync_state(&app_state)?;
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
        ui.sync_state(&app_state)?;
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
            ui.sync_state(&app_state)?;
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

    app_state.set_controller_phase(frame_core::ControllerPhase::Searching);
    frame_ui::set_controller_phase(app_state.controller_phase.clone())?;
    ui.sync_state(&app_state)?;

    tracing::info!(
        target: "frame_firmware",
        device_id = app_state.device_id.as_deref().unwrap_or_default(),
        default_controller_url = DEFAULT_CONTROL_PLANE_URL,
        "network connected; awaiting Home Assistant adoption support"
    );

    rom_print(b"frame-firmware: entering UI run loop\r\n\0");

    ui.run()?;
    rom_print(b"frame-firmware: ui.run() returned unexpectedly\r\n\0");
    anyhow::bail!("ui.run() returned unexpectedly")
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    tracing::warn!(
        "frame-firmware only runs on the espidf target; use `cargo firmware-check` or `cargo firmware-build` from the workspace root."
    );
}
