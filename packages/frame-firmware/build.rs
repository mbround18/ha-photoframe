fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=components/frame_embedded_ui");
    println!("cargo:rerun-if-changed=sdkconfig.defaults");
    println!("cargo:rerun-if-changed=../../sdkconfig.defaults");
    embuild::espidf::sysenv::relay();
    embuild::espidf::sysenv::output();
}
