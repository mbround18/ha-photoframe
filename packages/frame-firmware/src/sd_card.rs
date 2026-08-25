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

    log::info!("SD card mounted at {MOUNT_POINT} ({capacity_mb} MB)");
    SdStatus::Ready { capacity_mb }
}
