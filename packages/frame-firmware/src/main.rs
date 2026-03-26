// This is the main execution crate for the photo frame firmware.
// It imports all the other crates, wires up the hardware, and starts the
// application loops.

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
fn main() {
    rom_print(b"frame-firmware: entered Rust main\r\n\0");

    // This is the entry point of our application.
    //
    // For the first ESP32-P4 smoke test we keep startup minimal and only link
    // the required ESP-IDF patches before composing the current Rust layers.
    esp_idf_sys::link_patches();

    if let Err(err) = run() {
        rom_print(b"frame-firmware: run() returned error\r\n\0");
        eprintln!("fatal firmware startup error: {err:#}");

        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
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

    let mut app_state = frame_core::AppState::new();
    let mut provisioning_manager = frame_net::create_provisioning_manager();
    let photos_client = frame_api::create_photos_client();
    let mut ui = frame_ui::create_ui()?;
    rom_print(b"frame-firmware: UI adapter created\r\n\0");

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

    std::thread::sleep(std::time::Duration::from_millis(1500));

    app_state.begin_setup();
    ui.sync_state(&app_state)?;
    tracing::info!(
        target: "frame_firmware",
        phase = app_state.phase.as_str(),
        network = app_state.network_phase.as_str(),
        "setup screen rendered"
    );

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

    if app_state.network_phase == frame_core::NetworkPhase::Provisioning {
        app_state.set_network_phase(frame_core::NetworkPhase::Authorizing);
        ui.sync_state(&app_state)?;
        tracing::info!(
            target: "frame_firmware",
            network = app_state.network_phase.as_str(),
            "authorization phase started"
        );

        let auth_response = {
            let _span = tracing::info_span!("device_authorization").entered();
            frame_api::oauth::request_device_authorization()?
        };
        app_state.set_auth_info(
            auth_response.user_code.clone(),
            auth_response.verification_uri.to_string(),
        );
        ui.sync_state(&app_state)?;
        tracing::info!(
            target: "frame_firmware",
            user_code = auth_response.user_code,
            verification_uri = auth_response.verification_uri.as_str(),
            "authorization info updated"
        );

        let token = {
            let _span = tracing::info_span!("device_authorization_poll").entered();
            frame_api::oauth::poll_for_device_authorization(&auth_response)?
        };
        app_state.clear_auth_info();
        app_state.set_network_phase(frame_core::NetworkPhase::Connected);
        ui.sync_state(&app_state)?;
        tracing::info!(
            target: "frame_firmware",
            access_token = token.access_token,
            "authorization successful"
        );
    }

    if app_state.network_phase == frame_core::NetworkPhase::Connected {
        let recent_photos = {
            let _span = tracing::info_span!("photos_bootstrap").entered();
            photos_client.recent_photos()?
        };
        tracing::info!(
            target: "frame_firmware",
            count = recent_photos.len(),
            "photo client returned recent photos"
        );
        app_state.mark_ready();
    }

    ui.sync_state(&app_state)?;
    tracing::info!(
        target: "frame_firmware",
        phase = app_state.phase.as_str(),
        network = app_state.network_phase.as_str(),
        "ui state synchronized"
    );
    rom_print(b"frame-firmware: entering UI run loop\r\n\0");

    // 2. TODO: Initialize hardware peripherals from `esp-idf-hal`.
    //    - SPI bus for the display.
    //    - I2C bus for the touch controller.
    //    - GPIO pins for display chip select, reset, DC, and backlight PWM.

    // 3. TODO: Initialize the `frame-net` crate to provision WiFi.
    //    - Start BLE provisioning if no WiFi credentials are found.
    //    - Otherwise, connect to the saved WiFi network.

    // 4. TODO: Initialize the `frame-ui` crate.
    //    - Create the display and touch controller drivers.
    //    - Initialize the Slint UI.

    // 5. TODO: Initialize the `frame-api` crate.
    //    - Perform OAuth2 device authorization flow if needed.
    //    - Create the Google Photos API client.

    // 6. TODO: Initialize the `frame-core` state machine.

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
