//! Constitution Principle II, mechanically enforced.
//!
//! The frame stores exactly two secrets: the Wi-Fi credential and a Home
//! Assistant-issued frame token. Every third-party credential -- Google OAuth
//! tokens, refresh tokens, provider API keys -- lives in Home Assistant and
//! must never appear on the device (FR-008, FR-043).
//!
//! Principle II is marked NON-NEGOTIABLE, so it gets the same mechanical guard
//! that Principle III (provider isolation) and Principle VIII (no developer
//! chrome) already have. A grep-based test is crude, but it fails loudly the
//! moment someone reintroduces on-device auth, which is exactly when a human
//! reviewer is least likely to notice.

use std::fs;
use std::path::{Path, PathBuf};

/// Identifiers that imply the frame is holding a credential it should not.
const FORBIDDEN: &[&str] = &[
    "oauth",
    "refresh_token",
    "client_secret",
    "access_token",
    "api_key",
    "id_token",
    "photoslibrary",
    "photospicker",
];

/// The only secrets the frame is allowed to know about.
const ALLOWED_SECRET_NAMES: &[&str] = &["wifi_psk", "frame_token", "provisioning_password"];

fn workspace_packages_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR is packages/frame-core
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("frame-core must live under packages/")
        .to_path_buf()
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if path.is_dir() {
            // Skip build output and anything vendored in.
            if name == "target" || name == "vendor" || name.starts_with('.') {
                continue;
            }
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            // This test file necessarily contains the forbidden words.
            if path
                .file_name()
                .is_some_and(|f| f == "no_third_party_credentials.rs")
            {
                continue;
            }
            out.push(path);
        }
    }
}

#[test]
fn frame_holds_no_third_party_credentials() {
    let packages = workspace_packages_dir();
    let mut sources = Vec::new();
    rust_sources(&packages, &mut sources);

    assert!(
        !sources.is_empty(),
        "found no Rust sources under {} -- the scan is broken, not the code",
        packages.display()
    );

    let mut violations = Vec::new();

    for path in &sources {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };

        for (index, line) in contents.lines().enumerate() {
            let lowered = line.to_lowercase();

            // Comments may discuss the removed flow; only code counts.
            let trimmed = lowered.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("/*") {
                continue;
            }

            for needle in FORBIDDEN {
                if !lowered.contains(needle) {
                    continue;
                }
                if ALLOWED_SECRET_NAMES.iter().any(|ok| lowered.contains(ok)) {
                    continue;
                }
                violations.push(format!(
                    "{}:{}: contains `{needle}`\n    {}",
                    path.display(),
                    index + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Constitution Principle II violation: the frame must hold no third-party \
         credential. Home Assistant owns all provider auth (FR-008, FR-043).\n\n{}\n\n\
         If this is a false positive, narrow the match rather than deleting the test.",
        violations.join("\n")
    );
}
