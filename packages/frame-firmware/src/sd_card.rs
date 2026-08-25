//! The SD card, and being honest about it when it is missing.
//!
//! The card is where prepared photos live so the frame keeps showing pictures
//! through a Home Assistant restart or a network outage (Constitution
//! Principle VII). Without it the frame still runs, but only from the handful
//! of photos it holds in memory -- a real degradation, and one worth reporting
//! rather than silently absorbing.
//!
//! Reported to Home Assistant, never shown on the panel: an adopted frame
//! shows photos and nothing else (Principle VIII).
//!
//! Two things about this board make the card harder to bring up than it looks,
//! both learned from the manufacturer's own demo sources rather than guessed:
//!
//! * The card sits on **SDMMC slot 0, routed through the IO MUX**, so its pins
//!   are fixed in silicon. Driving it through the GPIO matrix with an explicit
//!   pin list is aimed at the wrong mechanism.
//! * Its power rail comes from an **on-chip LDO (channel 4)** that must be
//!   switched on first. Skip that and the card simply never answers -- which
//!   looks exactly like an empty slot, and is the failure this module is meant
//!   to distinguish.
//!
//! Both are handled by the vendored BSP's `bsp_sdcard_mount()`, which is why
//! this goes through the BSP rather than building an `sdmmc_host_t` here. An
//! earlier attempt at the latter linked cleanly and then jumped to address zero
//! on the first transaction: the struct is mostly function pointers that
//! ESP-IDF fills in via a `SDMMC_HOST_DEFAULT()` macro, and bindgen does not
//! surface macros.

use esp_idf_svc::sys;

/// Where the card is mounted, matching the BSP's `BSP_SD_MOUNT_POINT`.
pub const MOUNT_POINT: &str = "/sdcard";

/// Photos pushed down by Home Assistant. Ours to manage: we add, evict, and
/// clear this on a factory reset, so nothing an owner puts here would survive.
pub const HA_CACHE_DIR: &str = "/sdcard/ha";

/// Photos the owner copied onto the card themselves.
///
/// Never written or deleted by the frame. If this folder has photos in it they
/// take over the slideshow entirely, which is what makes the frame usable with
/// no Home Assistant and no network at all.
pub const MEDIA_DIR: &str = "/sdcard/media";

/// Explains the above to whoever plugs the card into a computer.
const MEDIA_README_NAME: &str = "READ ME.txt";

const MEDIA_README: &str = "\
YOUR PHOTOS GO IN THIS FOLDER\r\n\
=============================\r\n\
\r\n\
Copy any photos you like straight into this folder, put the card back in the\r\n\
frame, and turn it on. The frame will show them.\r\n\
\r\n\
While there are photos in here, the frame shows these and only these. It will\r\n\
not need Wi-Fi and it will not need Home Assistant -- it works completely on\r\n\
its own.\r\n\
\r\n\
To go back to photos chosen in Home Assistant, take every photo out of this\r\n\
folder and turn the frame off and on again.\r\n\
\r\n\
\r\n\
GOOD TO KNOW\r\n\
------------\r\n\
\r\n\
* JPEG and PNG photos work. Files ending .jpg .jpeg or .png.\r\n\
\r\n\
* iPhone photos are often .HEIC, which the frame cannot read. In Photos,\r\n\
  choose File > Export > Export Photo and pick JPEG.\r\n\
\r\n\
* Photos of any size and any shape are fine. Tall photos get black bars at\r\n\
  the sides rather than having their tops and bottoms cut off.\r\n\
\r\n\
* Folders inside this one are ignored. Photos need to sit directly in here.\r\n\
\r\n\
* The other folder on this card, 'ha', belongs to the frame. Please leave it\r\n\
  alone -- the frame empties it whenever it likes.\r\n\
";

/// What happened when we tried to bring the card up.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SdStatus {
    /// Mounted and usable.
    Ready { capacity_mb: u64 },
    /// No card in the slot, or it did not answer.
    NotPresent,
    /// Card present but unusable.
    Failed(String),
}

impl SdStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    /// Short, plain-language description for Home Assistant.
    pub fn summary(&self) -> String {
        match self {
            Self::Ready { capacity_mb } => format!("ready ({capacity_mb} MB)"),
            Self::NotPresent => "no card detected".to_string(),
            Self::Failed(reason) => format!("unusable: {reason}"),
        }
    }
}

/// Bring the card up.
///
/// Never returns an error: a missing card is a degraded state to report, not a
/// reason to stop the frame working.
pub fn mount() -> SdStatus {
    let result = unsafe { sys::bsp_sdcard_mount() };

    if result != sys::ESP_OK {
        // ESP_ERR_TIMEOUT and ESP_ERR_NOT_FOUND are what an empty slot looks
        // like; anything else means a card answered and then disappointed us.
        let status = match result {
            sys::ESP_ERR_TIMEOUT | sys::ESP_ERR_NOT_FOUND => SdStatus::NotPresent,
            other => SdStatus::Failed(format!("mount failed (esp_err {other})")),
        };
        log::warn!(
            "SD card unavailable: {}. The frame will run from its in-memory photo \
             buffer and will not keep photos across a reboot.",
            status.summary()
        );
        return status;
    }

    let capacity_mb = unsafe {
        let card = sys::bsp_sdcard_get_handle();
        if card.is_null() {
            0
        } else {
            // csd holds the capacity in sectors; sector size is bytes per sector.
            let csd = (*card).csd;
            (csd.capacity as u64) * (csd.sector_size as u64) / (1024 * 1024)
        }
    };

    if let Err(error) = prepare_layout() {
        // The card mounted, so the cache still works; only the owner-facing
        // folder is missing. Not worth downgrading the card's status for.
        log::warn!("SD card mounted but its folder layout could not be prepared: {error}");
    }

    log::info!("SD card mounted at {MOUNT_POINT} ({capacity_mb} MB)");
    SdStatus::Ready { capacity_mb }
}

/// Create the two top-level folders and the note explaining them.
///
/// The note is written only when absent, so an owner who edits or deletes it
/// does not have it silently restored on every boot.
fn prepare_layout() -> std::io::Result<()> {
    std::fs::create_dir_all(HA_CACHE_DIR)?;
    std::fs::create_dir_all(MEDIA_DIR)?;

    let readme = std::path::Path::new(MEDIA_DIR).join(MEDIA_README_NAME);
    if !readme.exists() {
        std::fs::write(&readme, MEDIA_README)?;
        log::info!("wrote {}", readme.display());
    }
    Ok(())
}
