# Quickstart & Validation Guide

**Feature**: `001-ha-managed-photo-frame` | **Date**: 2026-08-25

How to build, run, and prove this feature works. Each scenario names the spec criterion it
discharges, so "done" is evidence rather than opinion.

---

## Prerequisites

**Host**

```bash
bash ./scripts/bootstrap-env.sh      # ESP-IDF python env + ldproxy
uv sync                              # Python tooling
```

**Hardware**: JC8012P4A1C_I_W board, USB-C to the CH340C port, a FAT-formatted SD card in the TF
slot (a blank card is fine — the frame formats it if the mount fails).

**Home Assistant**: a dev instance on the same LAN. The Bluetooth integration must be enabled and
working for Improv BLE adoption.

---

## Build and flash

```bash
make build            # firmware, release
make flash            # flash over USB
make dev              # flash + serial monitor
make lint             # clippy on the host target
cargo test --workspace --target x86_64-unknown-linux-gnu   # host-testable crates
```

**Note**: `sdkconfig.defaults` currently hard-codes an absolute path to `partitions_16mb.csv` under
a directory that no longer exists. Milestone M0 fixes this; until then a fresh checkout will not
build.

## Install the integration

Development, from a checkout:

```bash
ln -s "$PWD/custom_components/photoframe_bridge" \
      "$HA_CONFIG/custom_components/photoframe_bridge"
```

Then restart Home Assistant. For the shipped path, add this repository to HACS as a custom
repository of type Integration and install from there (FR-044).

```bash
python -m script.hassfest --integration-path custom_components/photoframe_bridge
pytest tests/                        # uses pytest-homeassistant-custom-component
```

---

## Validation scenarios

### V1 — Adoption end to end (US1, SC-001, SC-002)

1. Erase the frame's NVS and power it on.
2. Home Assistant → Settings → Devices → the Improv BLE discovery card. Provision Wi-Fi.
3. Within ~30 s a `PhotoFrame …` zeroconf discovery card appears. Adopt it and name it.

**Pass**: a device with entities exists; the panel leaves setup for the bundled sample photos.
**Time it** — SC-001 allows 10 minutes end to end, SC-002 allows 2 minutes for adoption alone.

**If BLE is unavailable** (M2 spike failed): join the `PhotoFrame-XXXX` network, complete the
Wi-Fi-only page, and pick up from step 3.

### V2 — Selecting photos (US2, SC-008)

1. Device → Configure → choose a photo source.
2. `media_source`: browse to a local media folder. `google_photos_picker`: complete the Google
   consent, open the picker link, select photos, return.
3. Confirm.

**Pass**: the frame is showing the selected photos within 60 seconds (SC-008). For the picker, the
UI must state plainly that the selection is fixed until re-picked (research R2).

### V3 — Photos render correctly (US3, SC-003, SC-004)

Prepare a deliberately awkward test set: landscape, portrait, EXIF-rotated (all 8 orientations), a
50 MP original, a 200x150 thumbnail, a CMYK JPEG, a PNG with alpha, a progressive JPEG, and a video.

**Pass**:
- Every photo appears correctly oriented, with proportions preserved (SC-003).
- Portraits get the blurred-backdrop letterbox, not a crop through the subject (FR-022).
- The video never appears (FR-018); the unreadable file is skipped with no on-screen error (FR-029).
- Record a transition at 60 fps and confirm no flicker, tearing, or blank gap (SC-004).

**Also assert on the HA side**: every prepared photo re-parses as **baseline** JPEG. A progressive
output would fail silently on hardware, so this is a unit test, not just an eyeball check
(research R4).

### V4 — Resilience (US4, SC-005, SC-006, SC-007)

With a slideshow running:

1. `ha core restart` — the slideshow must not stutter.
2. Unplug the router for 30 minutes — the slideshow must continue from SD cache.
3. Restore the network — new photos must resume within 60 s with no user action (SC-006).
4. Pull the frame's power, restore it — photos must be back within 30 s, **before** it reaches the
   controller (SC-007).
5. Eject the SD card mid-slideshow — it must degrade to in-memory playback, not stop (FR-030).

**Evidence**: a continuous video across all five steps.

### V5 — Controls (US5, SC-010)

Exercise every entity: pause/resume, next/previous, brightness, interval, order, screen on/off, and
the "show this photo" service. Then drive the same controls from an automation.

**Pass**: each takes effect within 2 s (SC-010); settings survive a power cycle (FR-033).

### V6 — The provider seam (US6, SC-013)

```bash
pytest tests/providers/test_conformance.py     # runs against every registered provider
pytest tests/test_provider_isolation.py        # fails if a provider name leaks outside providers/
```

**Pass**: adding `providers/sample.py` required no change to the coordinator, config flow, entities,
protocol, or firmware — verifiable from the diff for that commit alone (SC-013).

### V7 — Reset and privacy (US7, SC-014)

1. Remove the config entry: the frame stops receiving photos and returns to discoverable.
2. Perform the on-device reset (corner hold, 10 s, confirm).
3. Dump NVS and mount the SD card on a PC.

**Pass**: no token, no Wi-Fi PSK, no controller reference, no photo bytes (SC-014, FR-042).
Confirm the gesture cannot be triggered by ordinary taps (FR-041).

### V8 — No developer chrome (SC-012, Principle VIII)

Enumerate every reachable display state of an **adopted** frame: normal playback, controller down,
network down, SD failed, photo decode failed, empty pool, screen off, mid-reset.

**Pass**: none shows an IP address, identifier, stack trace, error code, or version string. Record
the inventory in the milestone evidence — this criterion is only meaningful if the enumeration is
exhaustive.

### V9 — Scale (SC-011)

Point a provider at a source with 20,000+ photos.

**Pass**: browsing and configuration stay responsive; memory does not grow with pool size (the
provider yields lazily — see the provider contract).

### V10 — Soak (SC-016)

Run 7 days uninterrupted with a 5-minute rotation.

**Pass**: no reboot, no memory growth trend, no visible fault. Capture heap high-water marks from
the serial log.

---

## Milestone evidence checklist

Per the constitution, compiling is not evidence.

| Milestone | Evidence |
|---|---|
| M0 | Green CI: cargo, pytest, hassfest, HACS action |
| M1 | Photograph of an SD-loaded photo; serial decode timing |
| M2 | BLE scanner screenshot, or a written go/no-go for the fallback |
| M3 | V1 recording, timed |
| M4 | V5 results, manual and automated |
| M5 | V2 (media source) + V6 test output |
| M6 | V2 (Google picker) recording |
| M7 | V4 continuous recording |
| M8 | V7, V8 inventory, V10 soak log |

---

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| No zeroconf discovery card | Frame and HA on different VLANs, or mDNS blocked. Use manual host entry. |
| No Improv BLE card | HA's Bluetooth integration is off, or the M2 spike failed and the C6 has no BT firmware. |
| Photos load but never display | Progressive JPEG reached the hardware decoder. Assert `progressive=False`. |
| Photos are sideways | EXIF transpose skipped in the HA pipeline — the frame does not rotate by design. |
| Frame shows samples after configuring a source | The provider raised `SourceUnavailable`; check the source's health entity. |
| `401` on photo fetch | Token rotated by a config-entry reload; the frame should re-`hello`. |
