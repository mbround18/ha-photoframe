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
    embuild::espidf::sysenv::relay();
    embuild::espidf::sysenv::output();
}
