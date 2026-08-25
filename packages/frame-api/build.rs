fn main() {
    // The frame holds no third-party credentials, so no provider secrets are
    // baked into the binary (Constitution Principle II, FR-008).
    println!("cargo:rerun-if-changed=build.rs");
}
