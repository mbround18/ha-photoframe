# Implementation Plan: Home Assistant-Managed Digital Photo Frame

**Branch**: `001-ha-managed-photo-frame` | **Date**: 2026-08-25 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-ha-managed-photo-frame/spec.md`

## Summary

Turn the existing ESP32-P4 board into an appliance that shows photos chosen in Home Assistant, and
ship the Home Assistant side as a HACS-installable integration.

Home Assistant becomes the whole brain. A `photoframe_bridge` integration holds every credential,
talks to photo sources through a pluggable `PhotoProvider` seam, decides what shows next, and
prepares each photo — orientation, crop, resize to the panel's exact geometry, baseline JPEG — before
handing it over. The frame receives a control message over a local WebSocket, downloads the prepared
JPEG over local HTTP into an SD-card cache, hardware-decodes it, and pushes it to the panel through
the P4's PPA. Because every photo it holds is already display-ready, the frame keeps running its
slideshow with no controller at all.

Three things from Phase 0 shape the work more than the original brief anticipated. Home Assistant's
built-in Google Photos integration turns out to be upload-only, so the direct Google Photos Picker
provider is mandatory rather than optional. Picker selections are frozen at pick time and expire,
so "new album photos appear automatically" becomes a per-provider capability rather than a promise.
And the photo path must bypass Slint and LVGL entirely in favour of the P4's hardware JPEG decoder
and PPA — which is both faster and less code than the software renderer in the tree today.

## Technical Context

**Language/Version**: Rust (pinned via `rust-toolchain.toml`, `esp` channel) for firmware and shared
core; Python 3.13 for the Home Assistant integration (HA 2025.1+ floor).

**Primary Dependencies**:
*Firmware* — `esp-idf-svc`, `esp-idf-hal`, `esp-idf-sys` (ESP-IDF v5.5.3), `esp_driver_jpeg`,
`esp_driver_ppa`, `esp_lcd_jd9365`, `esp-hosted-mcu`, `slint` (setup screens only), `serde_json`,
`tungstenite`.
*Integration* — `homeassistant`, `aiohttp`, `Pillow` (already an HA dependency),
`zeroconf` (via HA's shared instance), HA's `application_credentials` and `config_entry_oauth2_flow`
helpers for the Google path.

**Storage**:
*Frame* — NVS for Wi-Fi credential, controller binding, and presentation settings; FAT on the SD
card (`/sdcard/photoframe/`) for the prepared-photo cache.
*Home Assistant* — config entries for credentials and selections; a disk cache under the config
directory for prepared photos.

**Testing**: `cargo test` on `x86_64-unknown-linux-gnu` for `frame-core`, `frame-api`,
`frame-ha-bridge`; `pytest` with `pytest-homeassistant-custom-component` for the integration;
`hassfest` and `hacs/action` in CI; on-hardware serial and photographic evidence per milestone.

**Target Platform**: ESP32-P4 (`riscv32imafc-esp-espidf`) on Jingcai JC8012P4A1C_I_W with an
ESP32-C6 radio co-processor; Home Assistant Core 2025.1+ on the same LAN.

**Project Type**: Embedded appliance plus a Home Assistant custom integration — a two-sided system
sharing one wire protocol.

**Performance Goals**: Photo decode-and-present under 100 ms (hardware JPEG is ~10 ms at 1280x800);
transitions at panel refresh with no tearing; control command acknowledged within 2 s (SC-010);
frame showing photos within 30 s of power-on without contacting the controller (SC-007); config flow
responsive against a 20,000-photo source (SC-011).

**Constraints**: The frame holds no third-party credential (Principle II). No developer text on an
adopted screen (Principle VIII). Prepared photos must be **baseline** JPEG — the hardware decoder
rejects progressive. Local network only at runtime. No blocking I/O on Home Assistant's event loop.
32 MB PSRAM total, of which two 1280x800 RGB565 framebuffers cost ~4 MB.

**Scale/Scope**: Several frames per Home Assistant instance; photo pools up to ~20,000 items; an
SD cache of ~500 prepared photos by default; three photo providers at ship.

## Constitution Check

*GATE: evaluated before Phase 0 and re-evaluated after Phase 1 design.*

| Principle | Gate | Status |
|---|---|---|
| I. std Rust on ESP-IDF | Firmware stays `esp-idf-*`, no `no_std` | **PASS** — no change to the toolchain. |
| II. HA is the only control plane | Frame stores only a Wi-Fi credential and a frame token | **PASS** — and strengthened: `frame-api/src/oauth.rs` and the portal's Google routes are deleted. |
| III. Pluggable provider model | Providers behind one interface; adding one touches nothing else | **PASS** — `PhotoProvider` ABC, three implementations, registry-based discovery. Enforced by an architecture test. |
| IV. Local-network-only runtime | Frame contacts nothing but its own HA | **PASS** — the frame never sees a `baseUrl`; HA fetches from Google. |
| V. Zero-config adoption, no app | Improv BLE → mDNS → config flow | **PASS with a documented fallback** — see Complexity Tracking; the fallback still requires no app. |
| VI. Frame renders, HA works | All decode/resize/encode in HA | **PASS** — and the frame's remaining decode is a hardware block, not an image pipeline. |
| VII. Frame keeps showing photos | SD cache spans outages | **PASS** — FR-023/026/027, cache-first boot path. |
| VIII. Consumer-grade screen | No status text once adopted | **PASS** — enforced by a display-state inventory test (SC-012). |
| IX. HA integration quality | Config flow, hassfest, no blocking I/O | **PASS** — requires migrating the existing YAML component to config entries. |

**Post-Phase-1 re-evaluation**: no new violations. The design added an architecture test for
Principle III and a display-state inventory for Principle VIII, both of which make previously
aspirational principles mechanically checkable. One item moved to Complexity Tracking (below).

## Project Structure

### Documentation (this feature)

```text
specs/001-ha-managed-photo-frame/
├── plan.md              # This file
├── spec.md              # Feature specification
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── control-protocol.md    # Frame <-> HA WebSocket + HTTP contract
│   ├── photo-provider.md      # The pluggable provider seam
│   └── discovery.md           # Improv BLE + mDNS/zeroconf contract
├── checklists/
│   └── requirements.md  # Spec quality checklist
└── tasks.md             # Created by /speckit-tasks, not by this command
```

### Source Code (repository root)

```text
custom_components/photoframe_bridge/     # MOVED here from packages/... for HACS (R7)
├── __init__.py                  # Config-entry setup/unload/reload
├── config_flow.py               # Zeroconf discovery, adoption, options, re-pick repair
├── const.py
├── coordinator.py               # Per-frame orchestration: pool, ordering, scheduling
├── control_server.py            # WebSocket control channel (from the existing controller.py)
├── protocol.py                  # Wire types, mirrors frame-core (existing, extended)
├── http_view.py                 # Authenticated prepared-photo endpoint
├── renderer.py                  # Pillow pipeline in an executor
├── photo_store.py               # Prepared-photo disk cache + content hashing
├── providers/
│   ├── __init__.py              # Registry, PhotoProvider ABC, capabilities
│   ├── media_source.py          # HA media sources
│   ├── google_photos_picker.py  # Direct OAuth + Picker API
│   └── sample.py                # Bundled photos; seam proof; pre-configuration content
├── entity.py                    # Shared device/entity base
├── image.py  sensor.py  switch.py  number.py  select.py  button.py
├── application_credentials.py   # Google OAuth wiring
├── diagnostics.py
├── strings.json  translations/en.json
├── services.yaml
└── manifest.json

hacs.json                        # HACS custom-repository descriptor (new, repo root)

packages/
├── frame-core/          # + provider-agnostic protocol types, cache policy, slideshow state
├── frame-api/           # - oauth.rs (deleted); + prepared-photo HTTP client
├── frame-net/           # + Improv BLE GATT service; + mDNS announcement
├── frame-ui/            # Slint setup screens only; photo path moves out
├── frame-firmware/
│   ├── src/
│   │   ├── photo_pipeline.rs    # NEW: hw JPEG decode -> PPA -> panel
│   │   ├── sd_cache.rs          # NEW: FAT mount, LRU cache, prefetch
│   │   ├── slideshow.rs         # NEW: cache-first playback, survives disconnection
│   │   └── control_client.rs    # NEW: WebSocket client, reconnect, claim
│   └── components/
│       └── frame_photo_render/  # NEW C shim: esp_driver_jpeg + PPA + panel flush
├── frame-captive-portal/        # Wi-Fi-only fallback; Google routes deleted
└── frame-ha-bridge/             # Rust protocol parser + PyO3 bindings (stays; source of truth)
```

**Structure Decision**: Keep the existing cargo workspace for firmware and add the three new
firmware modules plus one C shim, because the photo path needs ESP-IDF driver APIs that have no Rust
bindings. Move the Home Assistant component from `packages/frame-ha-bridge/homeassistant/` to a
repository-root `custom_components/` — HACS requires that location and will not install from a
nested path (R7). `frame-ha-bridge` keeps the Rust protocol parser and its PyO3 bindings as the
protocol's source of truth, with a CI test asserting `protocol.py` agrees with it.

## Implementation Milestones

Ordered by the constitution's Development Workflow. Each is independently verifiable, and each maps
to spec user stories.

| # | Milestone | Stories | Evidence required |
|---|---|---|---|
| M0 | Groundwork: fix the `sdkconfig.defaults` absolute path, move the component to the repo root, add `hacs.json`, wire hassfest + HACS + cargo CI, delete the on-device OAuth | — | Green CI; workspace builds; no `oauth` symbols remain |
| M1 | SD card + hardware photo path: mount FAT, hw JPEG decode, PPA present, cross-fade | US3 | Photograph of a correctly-oriented photo from the SD card; serial timing under 100 ms |
| M2 | ~~**Spike**: BLE GATT peripheral on P4 via ESP-Hosted~~ **DONE 2026-08-25 — PASS** | US1 | ✅ Advertises as `PhotoFrame-B566`, seen from an independent adapter at -43 dBm, accepts connections. [research.md](./research.md) R10 |
| M3 | Adoption: Improv (or fallback) provisioning, mDNS announcement, HA zeroconf config flow, frame token | US1 | Factory-fresh frame to adopted device on video, under 10 minutes |
| M4 | Control channel + entities: config-entry migration, WebSocket claim, image/sensor/switch/number/select/button | US5 | Every control exercised from HA and from an automation |
| M5 | Provider seam + `sample` + `media_source` providers, render pipeline, prepared-photo view | US2, US6 | Photos from a local media source on the panel; architecture test passes |
| M6 | Google Photos Picker provider: application credentials, picker session flow, re-pick repair | US2 | Owner picks photos in Google's picker; they appear on the frame |
| M7 | Resilience: SD LRU cache, prefetch, cache-first boot, reconnect backoff | US4 | HA restarted and network pulled for 30 minutes with no visible interruption |
| M8 | Polish and privacy: display-state inventory, on-device reset, diagnostics, translations, 7-day soak | US7 | SC-012 inventory signed off; reset verified; soak log |

## Key Design Decisions

Carried from research.md so tasks can be written without re-reading it.

1. **The frame never sees a provider URL.** Home Assistant fetches, prepares, and re-serves. This
   falls out of Google's 60-minute authenticated `baseUrl`, and it is what makes Principle II
   enforceable rather than aspirational.
2. **Providers declare capabilities.** `supports_live_collections`, `supports_collections`,
   `supports_individual_selection`, `selection_expires`. The coordinator branches on capability, not
   on provider identity — that is what keeps Principle III honest.
3. **Prepared photos are content-addressed and outlive their source.** A photo prepared once
   survives a Picker session expiring, so the frame does not go dark when a selection lapses.
4. **The frame is cache-first on boot.** It starts the slideshow from SD before it has a network,
   let alone a controller (SC-007).
5. **Two rendering paths, strictly separated.** Hardware JPEG + PPA for photos; Slint only for
   first-run screens. After adoption the Slint path is unreachable except for a fatal error card.
6. **Baseline JPEG is a contract term, not an implementation detail.** The hardware decoder rejects
   progressive JPEG, so `progressive=False` is tested on the Home Assistant side.
7. **One protocol, two languages, one source of truth.** `frame-core` defines the wire types in
   Rust; `frame-ha-bridge` exposes them to Python; `protocol.py` mirrors them dependency-free for
   HACS; CI asserts the mirror matches.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| ~~Two provisioning mechanisms~~ **RESOLVED — one mechanism** | The M2 spike passed on hardware (R10), so only Improv over BLE is built. The Wi-Fi-only SoftAP fallback (T059-T061) is cancelled and no extra code ships. | n/a — this entry is retained only to record that the contingency was planned, exercised, and closed. |
| A C shim component (`frame_photo_render`) alongside Rust firmware | `esp_driver_jpeg` and `esp_driver_ppa` have no Rust bindings, and the existing `frame_embedded_ui` C component already establishes this pattern in the repo. | Writing Rust bindings for both drivers: substantially more work and more unsafe surface than a ~200-line C shim with a narrow, testable interface. |

## Hardware Facts That Shaped This Plan

Established from the vendor bundle under `/source` and recorded in
[docs/Hardware-Reference.md](../../docs/Hardware-Reference.md):

- **BLE over SDIO works — verified on hardware.** The frame advertises, is discoverable from an
  independent Bluetooth adapter, and accepts connections, with Wi-Fi up simultaneously. GPIO54
  resets the C6, confirmed in the boot log. One caveat: the board's ESP-Hosted slave firmware is
  2.1.0 against a 2.12.0 host, so the newer BT-init RPCs return `ESP_ERR_NOT_SUPPORTED` and must be
  treated as non-fatal ([research.md](./research.md) R10).
- **The upstream BSP drives the wrong panel.** It installs an ILI9881C, which does not answer
  (`ID1: 0x0`). The board needs `esp_lcd_jd9365`. The frame boots and runs, but the display is not
  correctly driven — this now blocks milestone M1, not just polish.
- The touch controller is a **GSL3680** the stock BSP cannot drive — so the factory-reset gesture
  uses the **GPIO35 BOOT button**, not touch.
- There is a **WS2812 RGB LED on GPIO26**, giving the frame a status channel that keeps the panel
  free of developer text (Principle VIII).
- Backlight is **GPIO23**, touch reset **GPIO22** — `docs/SPECIFICATION.md` had both wrong and has
  been corrected.
- Touch, codec, RTC, and camera all share **one I2C bus on GPIO7/GPIO8**.
- The SD socket's VDD comes from **`ESP_LDO_VO4`**, which must be acquired before mounting.

## Deviations From The Original Brief

Recorded so they are decisions rather than drift. Both were forced by Phase 0 findings.

1. **The direct Google Photos provider is mandatory, not optional.** The brief assumed Home
   Assistant's built-in Google Photos support could supply albums; it cannot (R1). Cost: milestone
   M6 grows, and the owner must create a Google Cloud project — the same requirement Home
   Assistant's own Google integrations impose.
2. **The third seam-proving provider is `sample`, not an S3 stub.** A bundled photo set is testable
   in CI with no credentials and doubles as the content an adopted-but-unconfigured frame displays,
   so the frame is never blank. S3 stays a documented future provider; the seam is proven either
   way.
3. **FR-014 and SC-009 need narrowing.** "Photos added to an album appear automatically" cannot hold
   for Picker selections, which are frozen at pick time (R2). The design handles this with a
   capability flag and an honest UI, but the spec text still over-promises and should be amended.
   Flagged for `/speckit-analyze`.
