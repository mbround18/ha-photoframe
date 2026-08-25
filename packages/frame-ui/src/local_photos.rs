//! The slideshow that needs no network.
//!
//! Photos the owner copies into `media/` on the SD card take over the frame
//! entirely: while that folder has anything in it, Home Assistant's photos are
//! ignored. That is what lets the frame work with no Wi-Fi, no Home Assistant,
//! and no adoption -- plug the card in, switch it on, and it shows pictures.
//!
//! The cost of skipping Home Assistant is that nothing has prepared these
//! photos. They arrive at whatever size and orientation the camera produced,
//! so the frame decodes and fits them itself (see `frame_ui::fit`), which is
//! the one place the firmware does real image work rather than blitting what
//! it was handed.

use crate::fit::fit_to_panel;
use crate::rendered_image::{RenderedImage, push_rendered_image};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What the frame can actually decode.
///
/// Notably absent is HEIC, which is what an iPhone produces by default. There
/// is no HEIC decoder in the firmware, so those files are skipped and the note
/// on the card tells the owner to export as JPEG.
const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];

/// Cap on how many photos we will index.
///
/// Someone will eventually empty a phone onto this card. Indexing is cheap but
/// not free, and a slideshow does not become better past this point.
const MAX_PHOTOS: usize = 5_000;

/// How long each photo stays on screen.
///
/// Fixed rather than configurable: a frame running from the card has no Home
/// Assistant to configure it from, and this is not worth a settings file the
/// owner would have to hand-edit.
const ROTATION_INTERVAL: Duration = Duration::from_secs(30);

/// Photos found on the card.
#[derive(Clone, Debug, Default)]
pub struct LocalLibrary {
    photos: Vec<PathBuf>,
    /// Files present but not decodable, worth reporting so an owner who copies
    /// 200 HEIC files learns why nothing happened.
    skipped: usize,
}

impl LocalLibrary {
    pub fn is_empty(&self) -> bool {
        self.photos.is_empty()
    }

    pub fn len(&self) -> usize {
        self.photos.len()
    }

    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// Plain-language summary for Home Assistant.
    pub fn summary(&self) -> String {
        match (self.photos.len(), self.skipped) {
            (0, 0) => "no photos on card".to_string(),
            (0, skipped) => format!("{skipped} file(s) on card, none readable"),
            (found, 0) => format!("{found} photo(s) on card"),
            (found, skipped) => format!("{found} photo(s) on card, {skipped} unreadable"),
        }
    }
}

/// Index the owner's folder.
///
/// Only the top level: a folder of folders is a filing system, and quietly
/// walking into it would surprise someone who parked an album in there.
pub fn scan_local_photos(dir: &str) -> LocalLibrary {
    let mut library = LocalLibrary::default();

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            // An absent folder is the normal case with no card, not a fault.
            log::debug!("no local photo folder at {dir}: {error}");
            return library;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if is_supported(&path) {
            if library.photos.len() < MAX_PHOTOS {
                library.photos.push(path);
            }
        } else if !is_readme(&path) {
            library.skipped += 1;
        }
    }

    // Alphabetical, so the order is predictable and an owner can control it by
    // naming files. Shuffling would make "why is it not showing them in order"
    // a question we cannot answer.
    library.photos.sort();
    library
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            SUPPORTED_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

/// Our own note on the card should not count against the owner as an
/// unreadable file.
fn is_readme(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
}

/// Decode one photo and fit it to the panel.
pub fn load(path: &Path) -> Result<RenderedImage> {
    let decoded = image::ImageReader::open(path)
        .with_context(|| format!("could not open {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("could not identify {}", path.display()))?
        .decode()
        .with_context(|| format!("could not decode {}", path.display()))?
        .into_rgb8();

    let (image, treatment) = fit_to_panel(&decoded)?;
    log::debug!(
        "loaded {} as {:?} ({}x{} source)",
        path.display(),
        treatment,
        decoded.width(),
        decoded.height()
    );
    Ok(image)
}

/// Run the local slideshow forever.
///
/// Never returns: a frame showing the owner's own photos has nothing to fall
/// back to and nothing to wait for. A photo that fails to decode is skipped
/// and the show goes on (FR-029).
pub fn run_local_slideshow(library: LocalLibrary) {
    if library.is_empty() {
        return;
    }

    log::info!(
        "showing {} photo(s) from the card; Home Assistant photos are ignored while \
         the media folder has photos in it",
        library.len()
    );

    let mut index = 0usize;
    let mut consecutive_failures = 0usize;

    loop {
        let path = &library.photos[index % library.len()];
        index = index.wrapping_add(1);

        match load(path) {
            Ok(image) => {
                consecutive_failures = 0;
                if let Err(error) = push_rendered_image(image) {
                    log::warn!("could not queue {}: {error}", path.display());
                }
            }
            Err(error) => {
                log::warn!("skipping {}: {error}", path.display());
                consecutive_failures += 1;
                // Every photo failing means the folder is unusable rather than
                // one file being corrupt. Back off instead of spinning through
                // thousands of bad files as fast as the CPU allows.
                if consecutive_failures >= library.len() {
                    log::error!(
                        "none of the {} file(s) on the card could be displayed; \
                         retrying in a minute",
                        library.len()
                    );
                    std::thread::sleep(Duration::from_secs(60));
                    consecutive_failures = 0;
                }
                continue;
            }
        }

        std::thread::sleep(ROTATION_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_photo_extensions_regardless_of_case() {
        assert!(is_supported(Path::new("/m/a.JPG")));
        assert!(is_supported(Path::new("/m/b.jpeg")));
        assert!(is_supported(Path::new("/m/c.PnG")));
        assert!(!is_supported(Path::new("/m/d.heic")));
        assert!(!is_supported(Path::new("/m/e")));
    }

    #[test]
    fn our_own_note_is_not_counted_as_an_unreadable_photo() {
        assert!(is_readme(Path::new("/m/READ ME.txt")));
        assert!(!is_readme(Path::new("/m/holiday.heic")));
    }

    #[test]
    fn an_empty_library_reports_plainly() {
        assert_eq!(LocalLibrary::default().summary(), "no photos on card");
    }

    #[test]
    fn unreadable_files_are_reported_so_a_folder_of_heic_is_explicable() {
        let library = LocalLibrary {
            photos: vec![],
            skipped: 200,
        };
        assert_eq!(library.summary(), "200 file(s) on card, none readable");
    }

    #[test]
    fn a_missing_folder_is_not_an_error() {
        assert!(scan_local_photos("/definitely/not/a/real/path").is_empty());
    }

    /// The whole point of the feature: a real photo file on a real disk, with
    /// no Home Assistant anywhere, ends up as panel-sized pixels.
    #[test]
    fn a_photo_on_disk_becomes_a_panel_sized_image() {
        let dir = std::env::temp_dir().join("frame-ui-local-photos-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A portrait photo, so this also exercises the letterbox path an
        // owner's phone pictures will overwhelmingly take.
        let source = image::RgbImage::from_pixel(600, 900, image::Rgb([10, 200, 30]));
        let path = dir.join("holiday.jpg");
        source.save(&path).unwrap();

        // Files we cannot read, and our own note, must not be counted as photos.
        std::fs::write(dir.join("from-iphone.HEIC"), b"not really heic").unwrap();
        std::fs::write(dir.join("READ ME.txt"), b"instructions").unwrap();

        let library = scan_local_photos(dir.to_str().unwrap());
        assert_eq!(library.len(), 1, "only the JPEG is a usable photo");
        assert_eq!(library.skipped(), 1, "the HEIC counts, the note does not");
        assert_eq!(library.summary(), "1 photo(s) on card, 1 unreadable");

        let rendered = load(&path).unwrap();
        assert_eq!(rendered.width(), crate::PANEL_LOGICAL_WIDTH as u32);
        assert_eq!(rendered.height(), crate::PANEL_LOGICAL_HEIGHT as u32);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn photos_are_ordered_predictably_so_naming_files_controls_the_order() {
        let dir = std::env::temp_dir().join("frame-ui-local-photos-order");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let photo = image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]));
        for name in ["3-third.jpg", "1-first.jpg", "2-second.jpg"] {
            photo.save(dir.join(name)).unwrap();
        }

        let library = scan_local_photos(dir.to_str().unwrap());
        let names: Vec<String> = library
            .photos
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["1-first.jpg", "2-second.jpg", "3-third.jpg"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
