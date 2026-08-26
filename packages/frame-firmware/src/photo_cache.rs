//! Photos kept on the SD card, so the frame does not depend on the network.
//!
//! Two tiers, for two different reasons:
//!
//! * **Memory** holds the next few photos, already in the panel's pixel
//!   format, so a rotation or a tap is instant. Small: each one is 2 MB of
//!   PSRAM.
//! * **The card** holds far more, because it is enormous and photos are not.
//!   This is what lets the frame keep showing pictures through a Home
//!   Assistant restart, a router reboot, or a week of nobody looking at it
//!   (Constitution Principle VII).
//!
//! Photos are stored exactly as they arrive: raw RGB565, ready for the panel.
//! That trades disk for CPU, which is the right way round here -- the card has
//! tens of gigabytes and the CPU is a 400 MHz core that overflowed its stack
//! the last time it was asked to decode anything.
//!
//! Deliberately not durable across a reset. This is a cache: a corrupt or
//! half-written file is discarded rather than repaired, and the frame asks
//! Home Assistant for another.

#![cfg(target_os = "espidf")]

use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::{Path, PathBuf};

/// Bytes in one cached photo, fixed by the panel.
const PHOTO_BYTES: usize = frame_ui::PANEL_LOGICAL_WIDTH * frame_ui::PANEL_LOGICAL_HEIGHT * 2;

/// How many photos to keep on the card.
///
/// At 2 MB each this is about 100 MB, against tens of gigabytes available, so
/// the limit is not really storage. It is how far ahead it is worth asking
/// Home Assistant to prepare: enough to ride out a long outage and hours of
/// rotation, not so much that a change of album takes ages to take effect.
const TARGET_PHOTOS: usize = 48;

/// Never use more than this much of the card.
///
/// The rest belongs to the owner: their `media` folder lives on the same card,
/// and a filesystem with no slack is a filesystem that fails at writing rather
/// than at deleting. A cache is the thing that should yield.
const MAX_CARD_FRACTION_USED: f64 = 0.80;

fn cache_dir() -> &'static Path {
    Path::new(crate::sd_card::HA_CACHE_DIR)
}

/// Cached photos are named `<order>-<photo id>.rgb565`.
///
/// The order prefix is zero-padded so sorting names is ordering arrivals. The
/// photo id is Home Assistant's content hash, which makes a photo already on
/// the card recognisable without reading it -- so a reconnect does not fetch
/// and rewrite two megabytes we already have.
fn path_for(sequence: u64, photo_id: &str) -> PathBuf {
    cache_dir().join(format!("{sequence:012}-{photo_id}.rgb565"))
}

/// The photo id encoded in a cache filename, if it has one.
fn photo_id_of(path: &Path) -> Option<String> {
    path.file_stem()?
        .to_str()?
        .split_once('-')
        .map(|(_, id)| id.to_string())
}

/// Whether this exact photo is already on the card.
pub fn contains(photo_id: &str) -> bool {
    entries()
        .iter()
        .any(|path| photo_id_of(path).as_deref() == Some(photo_id))
}

/// The order prefix to use for the next photo written.
///
/// Derived from what is already on the card rather than from a counter that
/// starts at zero: the counter resets on every reboot, and photos cached
/// before the reboot would have been overwritten by photos cached after it.
pub fn next_sequence() -> u64 {
    entries()
        .iter()
        .filter_map(|path| {
            path.file_stem()?
                .to_str()?
                .split_once('-')
                .and_then(|(order, _)| order.parse::<u64>().ok())
        })
        .max()
        .map_or(0, |highest| highest + 1)
}

/// How many photos are waiting on the card.
pub fn count() -> usize {
    entries().len()
}

/// How many more the frame would like, given what it already holds.
///
/// Zero once the cache is full, so a frame with a deep reserve stops asking
/// and a controller with a large album is not pestered forever.
pub fn wanted() -> usize {
    TARGET_PHOTOS.saturating_sub(count())
}

/// Cached photos, oldest first.
///
/// Names are zero-padded sequence numbers, so sorting them is ordering them.
fn entries() -> Vec<PathBuf> {
    let Ok(dir) = fs::read_dir(cache_dir()) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rgb565"))
        .collect();
    found.sort();
    found
}

/// Write one photo to the card.
///
/// Written to a temporary name and renamed, so a power cut leaves a partial
/// file that is never mistaken for a photo.
pub fn store(photo_id: &str, pixels: &[u8]) -> Result<()> {
    ensure!(
        pixels.len() == PHOTO_BYTES,
        "refusing to cache {} bytes; a photo is {PHOTO_BYTES}",
        pixels.len(),
    );
    ensure!(
        !photo_id.is_empty() && photo_id.chars().all(|c| c.is_ascii_alphanumeric()),
        "refusing to cache under an unusable name {photo_id:?}",
    );

    if contains(photo_id) {
        // Already here. Rewriting it would cost two megabytes of card wear to
        // end up exactly where we started.
        return Ok(());
    }

    if !has_room() {
        // Not an error: the card being full is a reason to stop caching, not a
        // reason to stop showing photos.
        log::debug!("SD cache is full; not storing another photo");
        return Ok(());
    }

    let final_path = path_for(next_sequence(), photo_id);
    let temp_path = final_path.with_extension("part");
    fs::write(&temp_path, pixels)
        .with_context(|| format!("could not write {}", temp_path.display()))?;
    fs::rename(&temp_path, &final_path)
        .with_context(|| format!("could not finish writing {}", final_path.display()))?;
    Ok(())
}

/// Take the oldest cached photo, removing it from the card.
///
/// Returns None when the cache is empty. A file that is the wrong size is
/// discarded and the next one tried: a truncated photo is worse than no photo,
/// because it would be blitted as garbage.
pub fn take_oldest() -> Option<Vec<u8>> {
    for path in entries() {
        match fs::read(&path) {
            Ok(bytes) if bytes.len() == PHOTO_BYTES => {
                let _ = fs::remove_file(&path);
                return Some(bytes);
            }
            Ok(bytes) => {
                log::warn!(
                    "discarding {}: {} bytes, expected {PHOTO_BYTES}",
                    path.display(),
                    bytes.len()
                );
                let _ = fs::remove_file(&path);
            }
            Err(error) => {
                log::warn!("discarding unreadable {}: {error}", path.display());
                let _ = fs::remove_file(&path);
            }
        }
    }
    None
}

/// Empty the cache, e.g. when the album changes.
pub fn clear() {
    for path in entries() {
        let _ = fs::remove_file(&path);
    }
}

/// Whether there is room for another photo, leaving the card headroom.
fn has_room() -> bool {
    if count() >= TARGET_PHOTOS {
        return false;
    }
    match usage() {
        Some((total, free)) if total > 0 => {
            let used_after = total.saturating_sub(free) + PHOTO_BYTES as u64;
            (used_after as f64) < (total as f64) * MAX_CARD_FRACTION_USED
        }
        // If the card will not say, trust the photo count, which is the limit
        // that actually binds on a card this size.
        _ => true,
    }
}

/// Total and free bytes on the card, or None if it cannot be determined.
fn usage() -> Option<(u64, u64)> {
    use esp_idf_svc::sys;

    let mut total: u64 = 0;
    let mut free: u64 = 0;
    let mount = std::ffi::CString::new(crate::sd_card::MOUNT_POINT).ok()?;
    let result = unsafe { sys::esp_vfs_fat_info(mount.as_ptr(), &mut total, &mut free) };
    (result == sys::ESP_OK).then_some((total, free))
}
