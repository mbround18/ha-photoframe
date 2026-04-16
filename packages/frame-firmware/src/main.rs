// This is the main execution crate for the photo frame firmware.
// It imports all the other crates, wires up the hardware, and starts the
// application loops.

#[cfg(target_os = "espidf")]
use anyhow::Context;

#[cfg(target_os = "espidf")]
use esp_idf_svc::nvs::EspDefaultNvsPartition;

#[cfg(target_os = "espidf")]
mod ownership_store;

#[cfg(target_os = "espidf")]
mod setup_state_store;

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
struct LocalSetupEndpoint {
    host: String,
    local_url: String,
    ip_url: Option<String>,
    pairing_code: String,
}

#[cfg(target_os = "espidf")]
fn local_setup_state_from_app(
    app_state: &frame_core::AppState,
) -> frame_captive_portal::LocalSetupState {
    let status = match app_state.network_phase {
        frame_core::NetworkPhase::Authorizing => {
            "Almost there. Finish Google sign-in to unlock your frame.".to_string()
        }
        frame_core::NetworkPhase::Connected if app_state.phase == frame_core::AppPhase::Ready => {
            "Your frame is ready for photos.".to_string()
        }
        frame_core::NetworkPhase::Connected => {
            "Wi-Fi is connected. Finishing setup details now.".to_string()
        }
        frame_core::NetworkPhase::Provisioning => {
            "Join the frame's setup Wi-Fi to keep going.".to_string()
        }
        frame_core::NetworkPhase::Unprovisioned => "The frame is getting setup ready.".to_string(),
    };

    let detail = match app_state.network_phase {
        frame_core::NetworkPhase::Authorizing => {
            "Scan the QR code shown on the frame or open the frame's local page. Once that browser is verified, continue with Google Photos consent there and approval will sync back automatically."
                .to_string()
        }
        frame_core::NetworkPhase::Connected if app_state.phase == frame_core::AppPhase::Ready => {
            "You can close this page now. The frame should begin pulling in your library shortly."
                .to_string()
        }
        frame_core::NetworkPhase::Provisioning => {
            "Stay connected to the frame's temporary network while you choose your home Wi-Fi."
                .to_string()
        }
        _ => "This page is served directly by the frame on your local network, so nearby setup stays simple and private."
            .to_string(),
    };

    frame_captive_portal::LocalSetupState {
        status,
        detail,
        owner_email: app_state.google_user_email().map(ToOwned::to_owned),
        pairing_code: app_state.pairing_code.clone(),
        local_setup_url: app_state.local_setup_url.clone(),
        local_setup_ip_url: app_state.local_setup_ip_url.clone(),
        auth_verification_uri: app_state.auth_verification_uri.clone(),
        auth_user_code: app_state.auth_user_code.clone(),
        device_id: app_state.device_id.clone(),
        device_name: app_state.device_name.clone(),
    }
}

#[cfg(target_os = "espidf")]
fn restore_owner_session(
    app_state: &mut frame_core::AppState,
    owner_store: &ownership_store::OwnerStore,
) -> anyhow::Result<bool> {
    let Some(stored_session) = owner_store.load()? else {
        return Ok(false);
    };

    match frame_api::oauth::refresh_device_access_token(&stored_session.refresh_token) {
        Ok(token) => {
            let mut google_user = frame_api::oauth::fetch_account_profile(&token.access_token)?;

            if google_user.subject != stored_session.owner.subject {
                owner_store.clear()?;
                anyhow::bail!(
                    "stored owner mismatch after refresh: expected {}, got {}",
                    stored_session.owner.email,
                    google_user.email
                );
            }

            google_user.refresh_token = stored_session.refresh_token.clone();
            app_state.set_google_user(google_user.clone());
            app_state.set_access_token(token.access_token);
            tracing::info!(
                target: "frame_firmware",
                owner_email = google_user.email,
                "restored owner session from NVS"
            );
            Ok(true)
        }
        Err(error) => {
            tracing::warn!(
                target: "frame_firmware",
                "failed to restore owner session from refresh token: {error}"
            );
            Ok(false)
        }
    }
}

#[cfg(target_os = "espidf")]
fn complete_owner_sign_in(
    app_state: &mut frame_core::AppState,
    owner_store: &ownership_store::OwnerStore,
    token: &frame_api::oauth::DeviceAccessToken,
) -> anyhow::Result<()> {
    let mut google_user = frame_api::oauth::fetch_account_profile(&token.access_token)?;

    if let Some(existing_google_user) = app_state.google_user.as_ref() {
        if existing_google_user.subject != google_user.subject {
            anyhow::bail!(
                "frame is already owned by {} and cannot be reassigned to {} without reset",
                existing_google_user.email,
                google_user.email
            );
        }
    }

    let refresh_token = token
        .refresh_token
        .clone()
        .context("device authorization did not return a refresh token")?;
    google_user.refresh_token = refresh_token.clone();
    owner_store.save(&ownership_store::StoredOwnerSession {
        owner: google_user.clone(),
        refresh_token,
    })?;
    app_state.set_google_user(google_user.clone());
    app_state.set_access_token(token.access_token.clone());

    tracing::info!(
        target: "frame_firmware",
        owner_email = google_user.email,
        "owner sign-in completed and persisted"
    );
    Ok(())
}

#[cfg(target_os = "espidf")]
fn generate_pairing_code() -> String {
    use rand::Rng;

    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..=999_999))
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
fn is_google_photos_scope_error(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}");
    message.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT")
        || message.contains("insufficient authentication scopes")
}

#[cfg(target_os = "espidf")]
fn advertise_local_setup(pairing_code: String) -> LocalSetupEndpoint {
    let ip_url = frame_net::wifi::current_sta_ip()
        .map(|maybe_ip| maybe_ip.map(|ip| format!("http://{ip}/")))
        .unwrap_or_else(|error| {
            tracing::warn!(target: "frame_firmware", "unable to read station IP after Wi-Fi connect: {error}");
            None
        });
    let local_url = ip_url.clone().unwrap_or_default();
    let host = ip_url
        .as_deref()
        .and_then(|url| {
            url.trim_start_matches("http://")
                .trim_end_matches('/')
                .split(':')
                .next()
        })
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "unavailable".to_string());

    LocalSetupEndpoint {
        host,
        local_url,
        ip_url,
        pairing_code,
    }
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
    browser_verified: bool,
) {
    match store.save_checkpoint(checkpoint, app_state, browser_verified) {
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
    let owner_store = ownership_store::OwnerStore::new(nvs_partition.clone())?;
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
        false,
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
        false,
    );

    std::thread::sleep(std::time::Duration::from_millis(1500));

    app_state.begin_setup();
    restore_owner_session(&mut app_state, &owner_store)?;
    ui.sync_state(&app_state)?;
    tracing::info!(
        target: "frame_firmware",
        phase = app_state.phase.as_str(),
        network = app_state.network_phase.as_str(),
        "setup screen rendered"
    );
    persist_setup_checkpoint(
        &setup_state_store,
        if app_state.google_user.is_some() {
            setup_state_store::SetupCheckpoint::OwnerRestored
        } else {
            setup_state_store::SetupCheckpoint::SetupRendered
        },
        &app_state,
        false,
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
                frame_core::NetworkPhase::Authorizing => {
                    setup_state_store::SetupCheckpoint::AwaitingBrowserPair
                }
                frame_core::NetworkPhase::Unprovisioned => {
                    setup_state_store::SetupCheckpoint::SetupRendered
                }
            },
            &app_state,
            false,
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

    let mut local_setup_server = frame_captive_portal::create_local_setup_server()?;
    local_setup_server.start()?;
    local_setup_server.update_state(local_setup_state_from_app(&app_state))?;
    persist_setup_checkpoint(
        &setup_state_store,
        setup_state_store::SetupCheckpoint::NetworkConnected,
        &app_state,
        false,
    );

    if app_state.google_user.is_none() {
        let local_setup = advertise_local_setup(generate_pairing_code());
        app_state.clear_provisioning_details();
        app_state.set_local_setup_details(
            local_setup.host.clone(),
            Some(local_setup.local_url.clone()),
            local_setup.ip_url.clone(),
        );
        app_state.set_pairing_code(local_setup.pairing_code.clone());
        ui.sync_state(&app_state)?;
        local_setup_server.update_state(local_setup_state_from_app(&app_state))?;
        tracing::info!(
            target: "frame_firmware",
            setup_host = local_setup.host,
            setup_url = local_setup.local_url,
            setup_ip_url = local_setup.ip_url.as_deref().unwrap_or(""),
            pairing_code = local_setup.pairing_code,
            "local setup endpoint prepared"
        );
        persist_setup_checkpoint(
            &setup_state_store,
            setup_state_store::SetupCheckpoint::LocalSetupReady,
            &app_state,
            false,
        );

        app_state.set_network_phase(frame_core::NetworkPhase::Authorizing);
        ui.sync_state(&app_state)?;
        local_setup_server.update_state(local_setup_state_from_app(&app_state))?;
        tracing::info!(
            target: "frame_firmware",
            network = app_state.network_phase.as_str(),
            "authorization phase started"
        );
        persist_setup_checkpoint(
            &setup_state_store,
            setup_state_store::SetupCheckpoint::AwaitingBrowserPair,
            &app_state,
            false,
        );

        loop {
            if local_setup_server.pairing_verified()? {
                tracing::info!(target: "frame_firmware", "browser pairing verified; waiting for browser OAuth callback");
                persist_setup_checkpoint(
                    &setup_state_store,
                    setup_state_store::SetupCheckpoint::BrowserPairVerified,
                    &app_state,
                    true,
                );
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        persist_setup_checkpoint(
            &setup_state_store,
            setup_state_store::SetupCheckpoint::BrowserOAuthReady,
            &app_state,
            true,
        );

        let token = loop {
            if let Some(token) = local_setup_server.take_browser_access_token()? {
                tracing::info!(target: "frame_firmware", "browser OAuth callback completed");
                persist_setup_checkpoint(
                    &setup_state_store,
                    setup_state_store::SetupCheckpoint::BrowserOAuthCallbackReceived,
                    &app_state,
                    true,
                );
                break token;
            }

            std::thread::sleep(std::time::Duration::from_millis(250));
        };
        complete_owner_sign_in(&mut app_state, &owner_store, &token)?;
        persist_setup_checkpoint(
            &setup_state_store,
            setup_state_store::SetupCheckpoint::AuthorizationComplete,
            &app_state,
            true,
        );
    } else {
        tracing::info!(
            target: "frame_firmware",
            owner_email = app_state.google_user_email().unwrap_or_default(),
            "skipping device authorization because owner session was restored"
        );
    }

    app_state.clear_provisioning_details();
    app_state.clear_auth_info();
    app_state.set_pairing_code(String::new());
    app_state.set_network_phase(frame_core::NetworkPhase::Connected);
    ui.sync_state(&app_state)?;
    local_setup_server.update_state(local_setup_state_from_app(&app_state))?;
    tracing::info!(target: "frame_firmware", "authorization state complete");
    persist_setup_checkpoint(
        &setup_state_store,
        setup_state_store::SetupCheckpoint::AuthorizationComplete,
        &app_state,
        true,
    );

    if app_state.network_phase == frame_core::NetworkPhase::Connected {
        if let Some(access_token) = app_state.access_token.clone() {
            let photos_client = frame_api::GooglePhotosClient::new(access_token);
            match photos_client.list_albums() {
                Ok(albums) => {
                    tracing::info!(
                        target: "frame_firmware",
                        count = albums.len(),
                        "photo client returned albums"
                    );
                }
                Err(error) if is_google_photos_scope_error(&error) => {
                    tracing::warn!(
                        target: "frame_firmware",
                        "owner token does not include Google Photos scopes; current setup flow restored identity successfully but browser Photos consent is still missing: {error:#}"
                    );
                }
                Err(error) => return Err(error),
            }
        }

        app_state.mark_ready();
        persist_setup_checkpoint(
            &setup_state_store,
            setup_state_store::SetupCheckpoint::Ready,
            &app_state,
            true,
        );
    }

    ui.sync_state(&app_state)?;
    local_setup_server.update_state(local_setup_state_from_app(&app_state))?;
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
