use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=components/frame_embedded_ui/CMakeLists.txt");
    println!("cargo:rerun-if-changed=components/frame_embedded_ui/frame_embedded_ui.c");
    println!("cargo:rerun-if-changed=components/frame_embedded_ui/include/frame_embedded_ui.h");
    println!("cargo:rerun-if-changed=components/frame_ble_spike/CMakeLists.txt");
    println!("cargo:rerun-if-changed=components/frame_ble_spike/frame_ble_spike.c");
    println!("cargo:rerun-if-changed=components/frame_ble_spike/include/frame_ble_spike.h");
    println!("cargo:rerun-if-changed=idf_component.yml");
    println!("cargo:rerun-if-changed=sdkconfig.defaults");
    println!("cargo:rerun-if-changed=../../sdkconfig.defaults");

    // The frame normally learns its controller's address at adoption (T067).
    // Until that flow exists, HA_CONTROL_URL in the workspace .env lets a
    // development board be pointed at a Home Assistant instance directly.
    // This is a convenience, not a credential: it is an address on the local
    // network, and no secret is baked into the binary (Principle II).
    println!("cargo:rerun-if-env-changed=HA_CONTROL_URL");

    let workspace_env = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.env");
    println!("cargo:rerun-if-changed={}", workspace_env.display());

    if let Ok(iter) = dotenvy::from_path_iter(&workspace_env) {
        for item in iter.flatten() {
            let (key, value) = item;
            if key == "HA_CONTROL_URL" && env::var_os(&key).is_none() && !value.trim().is_empty() {
                println!("cargo:rustc-env={key}={value}");
            }
        }
    }

    embuild::espidf::sysenv::relay();
    embuild::espidf::sysenv::output();
}
