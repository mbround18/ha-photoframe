fn main() {
    println!("cargo:rustc-check-cfg=cfg(esp_idf_comp_mdns_enabled)");
    println!("cargo:rustc-check-cfg=cfg(esp_idf_comp_espressif__mdns_enabled)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=components/frame_embedded_ui/CMakeLists.txt");
    println!("cargo:rerun-if-changed=components/frame_embedded_ui/frame_embedded_ui.c");
    println!("cargo:rerun-if-changed=components/frame_embedded_ui/include/frame_embedded_ui.h");
    println!("cargo:rerun-if-changed=idf_component.yml");
    println!("cargo:rerun-if-changed=sdkconfig.defaults");
    println!("cargo:rerun-if-changed=../../sdkconfig.defaults");
    embuild::espidf::sysenv::relay();
    embuild::espidf::sysenv::output();
}
