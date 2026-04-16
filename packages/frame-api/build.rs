fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=GOOGLE_OAUTH_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=GOOGLE_OAUTH_CLIENT_SECRET");
    println!("cargo:rerun-if-env-changed=GOOGLE_OAUTH_REDIRECT_URI");
}
