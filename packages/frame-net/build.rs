use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=WIFI_SSID");
    println!("cargo:rerun-if-env-changed=WIFI_PASSWORD");

    let workspace_env = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.env");
    let workspace_env_sample = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.env.sample");
    println!("cargo:rerun-if-changed={}", workspace_env.display());
    println!("cargo:rerun-if-changed={}", workspace_env_sample.display());

    if let Ok(iter) = dotenvy::from_path_iter(&workspace_env) {
        for item in iter.flatten() {
            let (key, value) = item;
            if matches!(key.as_str(), "WIFI_SSID" | "WIFI_PASSWORD")
                && env::var_os(&key).is_none()
                && !value.trim().is_empty()
            {
                println!("cargo:rustc-env={key}={value}");
            }
        }
    }
}
