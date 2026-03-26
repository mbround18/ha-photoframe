// This is the root of the frame-net crate.
// It will handle BLE/WiFi provisioning and network state.

pub mod ble;
pub mod provisioning;
pub mod wifi;

pub use provisioning::{ProvisioningManager, create_provisioning_manager};
