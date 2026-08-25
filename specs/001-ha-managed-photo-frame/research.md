# Phase 0 Research: Home Assistant-Managed Digital Photo Frame

**Feature**: `001-ha-managed-photo-frame` | **Date**: 2026-08-25

This document resolves the unknowns in the plan's Technical Context. Five findings changed the
design materially and are marked **[DESIGN-CHANGING]**.

R1-R8 came from vendor and API documentation. **R9 came from the vendor hardware bundle under
`/source`** and corrected two pin errors plus one wrong driver assumption that were already baked
into the task list.

---

## R1. Home Assistant's built-in Google Photos integration cannot read the user's albums

**[DESIGN-CHANGING]**

**Finding**, corrected 2026-08-25 by reading Home Assistant's source rather than its docs:

The `google_photos` media source *is* a real album browser. It renders three levels -- accounts,
albums, then media items -- and calls `list_albums()` with no filtering of its own.

The limiter is the OAuth scope it requests:

```python
READ_SCOPE = "https://www.googleapis.com/auth/photoslibrary.readonly.appcreateddata"
```

`.appcreateddata` means Google returns only albums and media *that this app created*. So the
browsing machinery is genuine and the restriction is applied on Google's side, to the response.
That reconciles the code (which looks like a full browser) with the documentation ("does not give
Home Assistant access to your entire Google Photos library"). Both are accurate.

The root cause is upstream of Home Assistant: Google withdrew library-wide read access on
2025-03-31, which is also what ended the `Daanoz/ha-google-photos` custom integration.

**Decision, revised**: one generic `media_source` provider consumes *every* Home Assistant media
source, `google_photos` included. Nothing Google-specific is built unless and until the generic
path proves insufficient in practice, which costs one click to check.

The Picker provider is **deferred, not cancelled**. It remains the only route to photos that exist
solely in a user's Google library, but it is the escape hatch rather than the headline: it requires
the owner to create a Google Cloud project, its selections freeze at pick time and expire, and it
carries ongoing API risk. Building it speculatively would be reinventing a wheel for a case that
may not arise.

**Rationale**: The owner's core request is "the user selects an album or a series of photos" from
their own Google Photos. Only a provider we write can do that today.

**Consequence for the provider lineup**: the three initial providers become:

1. `media_source` — generic Home Assistant media sources. **This is the primary path.** It covers
   local media, Nextcloud, DLNA, Samba, `google_photos`, and most importantly **Immich**, whose
   media source groups assets by *albums, people and tags*. "Everything with these faces in it" is
   a better photo-frame album than anything hand-curated, and it stays current on its own. One
   provider, no per-source code, and a new media source in Home Assistant works here with no
   change (FR-016, SC-013).
2. `google_photos_picker` — direct OAuth + Picker API. **Deferred**: the escape hatch for photos
   that live only in a Google library, built only if the generic path proves insufficient.
3. `sample` — a built-in bundled photo set. Replaces the S3 stub as the seam-proving third
   provider: it is testable in CI with no credentials, and it doubles as the "frame has been
   adopted but not configured yet" content so the frame is never blank. S3 remains a documented
   future provider, not built here.

**Alternatives considered**: Waiting for core `google_photos` to gain library read (no evidence it
is planned, and the API it would need was withdrawn); using the legacy Library API directly
(scopes withdrawn 2025-03-31, unavailable to new clients).

---

## R2. Google Photos Picker: 60-minute URLs and a frozen selection

**[DESIGN-CHANGING]**

**Findings**, from Google's Picker API documentation:

| Fact | Consequence |
|---|---|
| Scope is `https://www.googleapis.com/auth/photospicker.mediaitems.readonly` | Narrow, read-only, picker-scoped. Good for Principle II. |
| Flow: `sessions.create` → user opens `pickerUri` → poll `sessions.get` until `mediaItemsSet` → `mediaItems.list` | The config flow must host a wait-and-poll step, honouring the server-supplied `pollingConfig`. |
| `baseUrl` is valid for **60 minutes** and requires an `Authorization: Bearer` header | The frame can never be given a `baseUrl`. Home Assistant must fetch bytes itself, promptly, and re-list to refresh URLs. This independently confirms the "HA prepares, frame downloads from HA" architecture. |
| Download requires an explicit size (`=w1280-h800`) or `=d` | We request `=w2560-h1600` — 2x the panel — and downscale locally, so crops stay sharp. Never `=d`; full originals are wasted bandwidth. |
| Sessions carry an `expireTime`; access to the session *and its picked items* ends then | The selection is not permanent. The integration must track `expireTime` and prompt for re-picking before it lapses. |
| The picked set is fixed at pick time | **FR-014 and SC-009 (new photos appear automatically) cannot hold for this provider.** |

**Decision**: Make "does this selection auto-update?" an explicit capability of the provider seam
rather than a global guarantee. `PhotoProvider.capabilities` reports `supports_live_collections`.
The `media_source` provider reports `True`; `google_photos_picker` reports `False`. The
integration surfaces this honestly in the UI ("This selection is fixed. Re-pick to add photos.")
and only schedules pool refreshes for providers that can benefit.

**Also decided**: because picked items expire with the session, the integration must treat its
prepared-photo store as the durable copy. Once a photo is prepared it survives session expiry, so
the frame keeps working even after the Google selection lapses — it just stops gaining new photos
until the owner re-picks. A repair flow (`async_step_reauth`-style) prompts them.

**Spec amendment required**: FR-014 and SC-009 must be scoped to providers that support live
collections. Flagged for `/speckit-analyze`; not silently ignored.

---

## R3. Improv over BLE is the right adoption UX, and the shipped C6 firmware supports it

> **Updated 2026-08-25 after examining `/source`.** The risk below has been substantially reduced.
> String extraction from the vendor's shipped C6 slave firmware (`JC-C6-slave_v2.3.2.bin`) shows it
> advertises `- WLAN over SDIO` **and `- HCI Over SDIO`**, and contains `slave_bt.c`,
> `esp_bt_controller_enable(ESP_BT_MODE_BLE)`, and the VHCI transport. BLE is therefore available
> **without reflashing the C6**. Additionally, P4 **GPIO54 drives the C6's `EN` pin**, so the host
> can hard-reset a wedged co-processor. The remaining unknown is only whether HCI-over-SDIO is
> *stable enough* in practice — no vendor demo enables `CONFIG_BT_ENABLED`, so this path is
> supported but unexercised by the vendor. The spike stands, but it is now "enable and verify"
> rather than "find out if it is possible". See [Hardware-Reference.md](../../docs/Hardware-Reference.md) §4.


**Finding**: Home Assistant ships an `improv_ble` integration (since 2023.11) that discovers
Improv-advertising devices via the Bluetooth integration and provisions them onto Wi-Fi. That is
exactly the "no companion app" flow FR-001 wants, and `improv-wifi.com` also offers a browser-based
provisioner over Web Bluetooth for users without a Bluetooth-equipped HA host.

**The risk**: the ESP32-P4 has no radio. BLE must run as a NimBLE host on the P4 with HCI
transported to the ESP32-C6 over SDIO by `esp-hosted-mcu`. Espressif documents that ESP-Hosted
"exposes a standard HCI interface to the Bluetooth host stack" and documents the P4+C6 pairing.
But this repo currently builds with `CONFIG_ESP_WIFI_REMOTE_ENABLED=y` and Wi-Fi only, and this is
a third-party board (Jingcai JC8012P4A1C) whose factory-flashed C6 slave firmware may be a
Wi-Fi-only build. Whether BLE HCI works here is **unproven on this hardware**.

**Decision**: Gate the adoption milestone on a spike (task `T-SPIKE-BLE`) that must produce a
connectable BLE GATT peripheral advertising from the P4 through the C6, evidenced by a serial log
plus a phone scanner screenshot. Two outcomes, both pre-planned:

- **Spike passes** → implement the Improv BLE GATT service. Preferred path.
- **Spike fails** (C6 slave firmware lacks BT, or HCI-over-SDIO is unstable) → fall back to a
  **Wi-Fi-only SoftAP setup page**: the frame raises an AP, serves one page that does nothing but
  join a network, then tears the AP down permanently. This reuses `frame-captive-portal`, which
  already exists, stripped to Wi-Fi. It shows a portal exactly once during first-run setup and
  never again, which is consistent with Principle VIII (that principle governs the *adopted* frame)
  and with FR-001, which requires no companion app — not the absence of any setup page.

The fallback is deliberately specified now so a failed spike costs a day, not a redesign. Note the
spike is also a prerequisite for re-flashing the C6 slave firmware if that turns out to be needed —
budget for that in the spike, and capture the stock C6 firmware before overwriting it.

**Alternatives considered**: Improv over Serial (requires a USB cable and a browser — worse UX than
SoftAP and no better than BLE); ESP SoftAP provisioning with the Espressif phone app (violates "no
companion app"); shipping pre-provisioned credentials (violates Principle V and the gift use case).

---

## R4. The photo path must bypass Slint and LVGL entirely

**[DESIGN-CHANGING]**

**Findings**:

- The ESP32-P4 has a **hardware JPEG decoder** (`esp_driver_jpeg`). Baseline JPEG only (not
  progressive), YUV444/422/420 subsampling, direct RGB565 output, ~109 fps at 1280x720 — so a
  1280x800 decode lands around 8-10 ms. Buffers must be allocated with `jpeg_alloc_decoder_mem()`
  for alignment, and output dimensions pad up to 16-byte multiples.
- The P4 also has a **PPA** (Pixel Processing Accelerator) for hardware scale / rotate / mirror.
  The vendor's own `video_lcd_display` demo drives the panel through `ppa_do_scale_rotate_mirror`.
- The current firmware renders through Slint's **software** renderer into an RGB565 buffer and
  performs a **software 270-degree rotation** into a second full framebuffer
  (`frame-ui/src/display.rs`, `frame_embedded_ui.c`). At 1280x800x2 bytes that is ~2 MB per buffer
  and a full CPU-side rotate per frame.
- The panel is natively 800x1280 portrait; the vendor BSP presents it as 1280x800 landscape.

**Decision**: Split rendering into two entirely separate paths.

- **Photo path (the 99.9% case)**: hardware JPEG decode → PPA scale/rotate → DMA straight to the
  DSI framebuffer. No Slint, no LVGL, no CPU-side rotation, no software renderer. Transitions are
  done by decoding the next photo into a second PSRAM framebuffer and cross-fading, which PPA's
  blend path or a tight RGB565 lerp can sustain.
- **Setup path (first-run only, and never again after adoption)**: keep the existing Slint/LVGL
  surface for the handful of first-run and fatal-error screens, where a 200 ms redraw is fine.

**Rationale**: SC-004 demands no flicker, tearing, or blank gaps. Pushing photos through a software
renderer and a CPU rotate cannot meet that, and it would burn most of PSRAM bandwidth. Going
straight to the hardware blocks the silicon already has is both faster and *less* code.

**Constraint this places on Home Assistant**: the integration MUST emit **baseline** JPEG (Pillow:
`progressive=False`), pre-rotated to the panel's orientation, at exactly the frame's advertised
geometry. Progressive JPEG will not decode in hardware. This becomes a contract requirement and a
test.

**Alternatives considered**: decoding on the frame with a software JPEG library (10-20x slower,
blows the transition budget); having the frame do the rotation (PPA does it free); sending raw
RGB565 (2 MB/photo over Wi-Fi versus ~200 KB — 10x the transfer time for no gain).

---

## R5. SD card wiring is confirmed and does not collide with the Wi-Fi co-processor

**Finding**: confirmed against **both** the vendor BSP and the board schematic (`4_CONN.png`,
`3_ESP32-P4.png`). The TF slot is on a dedicated 4-bit SDMMC bus:

| Signal | GPIO |
|---|---|
| D0 | 39 |
| D1 | 40 |
| D2 | 41 |
| D3 | 42 |
| CLK | 43 |
| CMD | 44 |

The ESP32-C6 co-processor uses the separate SD2 bus on **GPIO14-GPIO19**. **No pin conflict** — SD
and Wi-Fi run simultaneously.

**One extra requirement found in the schematic**: the TF socket's `VDD` is fed from `ESP_LDO_VO4`,
an internal P4 LDO channel. That rail must be brought up before mounting, or the card simply will
not enumerate. All six signal lines carry 5K1 pullups.

**Decision**: Mount the card as FAT via `esp_vfs_fat_sdmmc_mount` in 4-bit mode at
`/sdcard`. Enable `format_if_mount_failed` so a blank or foreign card self-heals rather than
bricking the slideshow (FR-030). Cache lives under `/sdcard/photoframe/`.

**Cache sizing**: prepared photos at 1280x800 baseline JPEG q85 run roughly 150-250 KB. A 2,000-
photo cache is therefore ~400 MB — trivial on a 64 GB card. The binding constraint is not capacity
but FAT directory performance and the pool refresh cost, so the cache is capped by **count**
(default 500, configurable) rather than by bytes, with LRU eviction.

---

## R6. Existing repository state

**Findings from the working tree**:

- The workspace already has the right crate seams: `frame-core` (state, control protocol),
  `frame-api`, `frame-net` (Wi-Fi, provisioning), `frame-ui` (Slint + display/input),
  `frame-firmware` (composition root), `frame-captive-portal`, `frame-ha-bridge`.
- `frame-ha-bridge` **already contains a working skeleton** of the Home Assistant component at
  `homeassistant/custom_components/photoframe_bridge`: manifest, const, a WebSocket controller, a
  `protocol.py` mirroring the Rust control types, and three services. `make package` already builds
  a distributable tarball. This is a real head start on the control channel.
- The control protocol already models render requests, device commands, transitions, brightness,
  correlation IDs, health status, and a `ControllerRegistration` claim — most of what the control
  channel needs.
- `frame-api/src/oauth.rs` (612 lines) and the Google portions of `frame-captive-portal` implement
  the on-device device-code flow that this feature removes.

**Decisions**:

- **Extend, do not rewrite,** the existing `photoframe_bridge` component and the `frame-core`
  control protocol. The delta is: a config flow (it is currently YAML/`async_setup` only), entity
  platforms, the provider layer, the render pipeline, and zeroconf discovery.
- The component must move from `async_setup` + `CONFIG_SCHEMA` to **config entries**
  (`async_setup_entry` / `async_unload_entry`), which FR-044 and Principle IX require. The
  hand-rolled WebSocket server on port 8765 stays as the control channel but becomes owned by the
  config entry rather than by global YAML.
- Delete `frame-api/src/oauth.rs`, its tests, and the Google routes in `frame-captive-portal`
  (per the owner's decision and Principle II).

**Bug found in passing** (not caused by this feature, but it will block any fresh checkout):
`sdkconfig.defaults` hard-codes `CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="/home/mbruno/development/photoframe/partitions_16mb.csv"` — an absolute path to a directory that no longer exists (the repo is
`ha-photoframe`). Fix to a relative path as a setup task.

---

## R7. Distribution via HACS

**Decision**: Ship as a HACS **custom repository** (not the default store, which requires a
separate submission and a track record). This needs:

- `hacs.json` at the repository root declaring the integration name and HA version floor.
- The integration at `custom_components/photoframe_bridge/` **from the repository root**.
- A `manifest.json` with a real `documentation` and `issue_tracker` URL, `config_flow: true`,
  `zeroconf`/`bluetooth` discovery keys, and a version matching the GitHub release tag.
- GitHub releases with semantic-version tags.

**Conflict with the current layout**: the component lives at
`packages/frame-ha-bridge/homeassistant/custom_components/photoframe_bridge`, but HACS requires it
at the repo root. Options weighed: (a) move the component to a root `custom_components/`,
(b) publish it from a separate repository, (c) keep it nested and generate a root copy in CI.

**Decision: (a) — move it to `custom_components/photoframe_bridge/` at the repository root.** It is
the only layout HACS supports without a second repository, it keeps firmware and integration
versioned together (they share a protocol contract, so lockstep is a feature), and it removes the
`make package` tarball step in favour of ordinary HACS installation. `frame-ha-bridge` keeps its
Rust/PyO3 parser and remains the source of truth for protocol types; the component keeps its
dependency-free `protocol.py` mirror, with a CI test asserting the two agree.

**Also required by Principle IX**: `hassfest` and `hacs/action` must run in CI on every push.

---

## R8. Photo preparation in Home Assistant

**Decision**: Pillow, which Home Assistant already depends on, in an executor thread. Never on the
event loop (Principle IX).

**Pipeline**, per photo: fetch bytes → `ImageOps.exif_transpose` (FR-020) → convert to RGB →
fit to the frame's geometry → encode baseline JPEG q85, `progressive=False` (required by R4) →
write to `.storage`-adjacent disk cache keyed by a content hash → serve via a registered HTTP view.

**Portrait-on-landscape treatment** (FR-022): a blurred, darkened, zoomed copy of the photo itself
as the backdrop with the full photo letterboxed sharp on top — the treatment phone photo apps and
TV screensavers use. Chosen over plain black bars (dead space on a 10" panel), over centre-cropping
(cuts heads off portraits — the exact failure FR-022 names), and over pairing two portraits
side-by-side (needs a second photo ready and looks like a contact sheet, not a frame).

**Serving**: an authenticated `HomeAssistantView` at `/api/photoframe_bridge/photo/{photo_id}`,
with each frame authenticating using the token it received at adoption. Signed paths were
considered and rejected: they expire, and the frame may fetch long after the URL was pushed.

---

---

## R10. T056 spike result: BLE works on this board — PASS

**Run on hardware 2026-08-25** against the board on `/dev/ttyACM0` (ESP32-P4 rev v1.3,
MAC `80:f1:b2:d0:b5:66`).

### Outcome: PASS. Take Branch A (Improv over BLE). The SoftAP fallback is not needed.

Evidence, in order:

1. **The co-processor advertises the capability.** ESP-Hosted's transport handshake prints:

   ```
   transport: capabilities: 0xd
   transport: Features supported are:
   transport:      * WLAN
   transport:        - HCI Over SDIO
   transport:        - BLE only
   ```

2. **The NimBLE host runs on the P4 and syncs with the controller on the C6.**

   ```
   frame_ble_spike: NimBLE host task started
   frame_ble_spike: BLE host synced; address 98:88:e0:7a:21:a6
   frame_ble_spike: advertising as "PhotoFrame-B566"
   ```

   The BLE address `98:88:e0:...` is distinct from the P4's Wi-Fi MAC `80:f1:b2:...`,
   confirming the radio really is the C6's.

3. **Independently visible from another radio.** A `bluetoothctl le` scan from the
   development machine (adapter `2C:0D:A7:AE:64:0A`) sees it at -43 to -51 dBm:

   ```
   [NEW] Device 98:88:E0:7A:21:A6 PhotoFrame-B566
   ```

4. **It accepts connections**, which is what Improv requires:

   ```
   frame_ble_spike: BLE connect: status=0
   ```

5. **Stable.** Zero reboots across the capture window, with Wi-Fi connected at the same time —
   so BLE and Wi-Fi coexist over the one SDIO link.

### The one real caveat: the C6 slave firmware is old

```
W: Version mismatch: Host [2.12.0] > Co-proc [2.1.0] ==> Upgrade co-proc to avoid RPC timeouts
```

The board ships ESP-Hosted slave firmware **2.1.0** while the host component is **2.12.0**.
Consequence: `esp_hosted_bt_controller_init()` / `..._enable()` return `ESP_ERR_NOT_SUPPORTED`,
because those RPCs are new. Older slaves bring their BT controller up automatically at boot, so
**treating that error as non-fatal and proceeding straight to `nimble_port_init()` works** — that
is exactly what the spike does.

**Decision**: do not depend on the 2.12-era BT RPCs. Treat `ESP_ERR_NOT_SUPPORTED` from them as
"controller already up" and continue. Revisit only if a concrete problem appears.

Upgrading the co-processor is possible but not required. Two paths, recorded for later:

- **ESP-Hosted slave OTA over the existing SDIO link** — no extra hardware. The full slave project
  ships in the `esp_hosted` component (`slave/`, with `partitions.esp32c6.csv`), and there is a
  `host_performs_slave_ota` example. Whether slave 2.1.0 implements the OTA RPC is untested.
- **Direct UART flashing via header CN5**, which carries `VCC3V3`, `GND`, `C6_U0TXD`, `C6_U0RXD`,
  `C6_IO9` (boot strap) and `C6_CHIP_PU`. Needs physical access and a USB-UART adapter.

The vendor's `JC-C6-slave_v2.3.2.bin` is also older than 2.12, so flashing it would not close the
version gap.

### Also confirmed on hardware

- **P4 GPIO54 is the co-processor reset line**, exactly as the schematic said:
  `sdio_wrapper: GPIOs: CLK[18] CMD[19] D0[14] D1[15] D2[16] D3[17] Slave_Reset[54]`
- The SDIO pin map in `sdkconfig.defaults` is correct.

### Consequence for the task list

- `T056` **PASS**, `T057` resolved: build **Branch A** (`T058`, Improv BLE GATT service).
- `T059`-`T061` (the Wi-Fi-only SoftAP fallback) are **not needed** and should not be built.

---

## R9. Board bring-up facts from the vendor bundle

**[DESIGN-CHANGING]**

Full detail in [docs/Hardware-Reference.md](../../docs/Hardware-Reference.md), derived from the
schematics in `/source/.../5-Schematic/` and the vendor's IDF 5.5.4 demos. The findings that change
tasks:

### The touch controller is a GSL3680, not the GT911 the stock BSP assumes

This repo pulls the upstream `espressif/esp32_p4_function_ev_board` BSP (5.2.3), which initialises a
GT911. This board has a **GSL3680**, and the vendor ships a custom `esp_lcd_touch_gsl3680` component
to drive it. **Touch does not work with the stock BSP.**

**Decision**: do not put the factory-reset gesture on touch. The board has a real **BOOT button on
GPIO35** (active low, 10K pullup, momentary to ground) that is readable from firmware. That is a
better reset trigger on every axis: it works without a vendored driver, it cannot be triggered by
dusting the screen, and it is a physical action a person can be told to perform over the phone.
Vendoring the GSL3680 driver becomes optional polish rather than a prerequisite for FR-040/FR-041.

### There is an addressable RGB LED on GPIO26

A single WS2812 on `WS2812_DAT`. This is a gift for Principle VIII: setup progress, connection
state, and reset confirmation can all be signalled **without putting any text on the panel**. It
gives the frame a status channel that a photo frame is allowed to have.

### Two pin errors in `docs/SPECIFICATION.md`

That document read the package-pin column as GPIO numbers. Corrected there, and it now points at
the hardware reference:

| Claim | Actual | Note |
|---|---|---|
| Backlight `GPIO25` | **GPIO23** | GPIO23 is on package pin 25 |
| Touch reset `GPIO24` | **GPIO22** | GPIO22 is on package pin 24 |

Task T042 said "backlight PWM control on GPIO25" and would have driven a USB pin. Corrected.

### Touch, codec, RTC, and camera share one I2C bus

The codec schematic carries an explicit net-alias table: `ES_I2C_SDA = RTC_DAT/SDA1 = GPIO7` and
`ES_I2C_SCL = RTC_CLK/SCL1 = GPIO8`. The apparent "second I2C bus for touch" in the specification
document does not exist. Anything added to that bus contends with the codec and the RTC.

### The display is portrait-native and the PPA rotates it for free

Panel is 800x1280 with a JD9365 driver over 2 DSI lanes at 1000 Mbps, DPHY powered from internal LDO
channel 3 at 2500 mV. The vendor's `video_lcd_display` demo drives the panel through
`ppa_do_scale_rotate_mirror()`. This confirms R4: rotation belongs in the PPA, not in the CPU-side
rotate the current firmware performs.

### The SD rail needs powering before mounting

The TF socket's `VDD` comes from `ESP_LDO_VO4`. Acquire that LDO channel before
`esp_vfs_fat_sdmmc_mount`, or the card never enumerates.

### Battery level is readable on GPIO52

Divider R2 68K / R6 100K from `BAT+`. Out of scope for a mains-powered frame, but it means an
unplugged frame could report "running on battery" rather than silently dying — worth a future task.

---

## R11. Display bring-up: the panel revision, and dropping Slint

**Resolved on hardware 2026-08-25.**

### The display fault was a panel-revision mismatch, not software

This board needs the **Old_Panel** JD9365 init sequence (197 command entries);
we were sending New_Panel's (204). Every API call returned `ESP_OK`, the panel
answered ID queries, and the backlight lit -- but nothing rendered. The
manufacturer's own factory image reproduces the fault, which is what finally
separated "our bug" from "wrong variant". See
[Hardware-Reference.md](../../docs/Hardware-Reference.md) section 4b.

**Method note worth keeping**: flashing the vendor image is a two-minute test
that would have short-circuited a long software hunt. When a device-level
symptom could be either our code or the hardware, reach for the vendor's
known-good binary early.

### The device is now Rust-only, and Slint is gone

Slint cost **2.15 MB** of flash -- the binary was 7,456,076 bytes against a
7,340,032-byte app partition, i.e. already too big to flash. Replacing it with
embedded-graphics, following ha-kiosk's model, brings it to 5,302,704 bytes with
2 MB spare.

Removed with it: the ~500-line `frame_embedded_ui.c` shim, `adapter.rs`,
`display.rs`, four `.slint` files, `slint-build`, `mipidsi`, `ft6x06`,
`qrcodegen`, and the Slint touch module.

**Decision**: the board BSP is vendored from the manufacturer's demo bundle and
bound into Rust with esp-idf-sys' `bindings_header`, exactly as ha-kiosk does.
The registry BSP targets Espressif's reference board -- it installs an ILI9881C
here and configures the DSI link differently, reporting success and lighting
nothing. Hand-rolling the DSI setup from the schematic was worse still: it meant
re-deriving the lane count, lane bit rate and colour order, and a wrong one is
invisible.

`esp_lcd_touch_gsl3680` is deliberately **not** vendored: it ships with no
licence or SPDX header. Touch is unused, and the factory-reset gesture uses the
BOOT button on GPIO35 (R9).

### Python now exists only where the platform requires it

`frame-ha-bridge` (PyO3) is deleted. It exposed protocol types to Python, but
`protocol.py` already mirrors them dependency-free, and it was the sole reason
for the libpython linking workarounds in the Makefile and CI.

The remaining Python is `esptool`, the pytest suite, and the HACS integration --
which is Python because Home Assistant loads Python modules. A fully Rust
alternative exists (ha-kiosk talks to HA's API directly with no custom
component) but it cannot hold Google Photos credentials server-side or prepare
photos off-device, so it breaks Principles II and VI.

---

## Resolved unknowns summary

| # | Unknown | Resolution |
|---|---|---|
| R1 | Does HA's `google_photos` expose the user's albums? | **No** — upload-only. We must write the Picker provider. |
| R2 | Picker session and URL lifetimes | 60-min `baseUrl`, session `expireTime`, selection frozen. Capability flag + re-pick repair flow. |
| R3 | Is Improv BLE achievable on P4+C6? | Documented as supported; unproven on this board. Spike-gated with a SoftAP fallback. |
| R4 | How do photos reach the panel? | Hardware JPEG + PPA, bypassing Slint/LVGL. HA must emit **baseline** JPEG. |
| R5 | SD card pins, conflicts, capacity | GPIO 39-44, no conflict with C6. Count-capped LRU cache at `/sdcard/photoframe/`. |
| R6 | What already exists? | Control protocol and HA component skeleton exist — extend them. On-device OAuth gets deleted. |
| R7 | HACS layout | Move component to root `custom_components/`. hassfest + HACS action in CI. |
| R8 | Image preparation | Pillow in an executor; blurred-backdrop letterbox for portraits; authenticated HTTP view. |
| R11 | Why did the display never render? | **Wrong panel revision.** This board needs the Old_Panel JD9365 init sequence. Also: Slint dropped (2.15 MB, was over the partition), BSP vendored and bound into Rust, PyO3 deleted. |
| R10 | Does BLE work on this board? | **Yes, verified on hardware.** Advertises, is discoverable from an independent radio, and accepts connections. Branch A confirmed; SoftAP fallback dropped. |
| R9 | Board pinout and bring-up | Touch is GSL3680 (stock BSP is wrong) → use the **GPIO35 BOOT button** for reset. Backlight is **GPIO23**, touch reset **GPIO22** (spec doc corrected). WS2812 status LED on GPIO26. SD rail needs `ESP_LDO_VO4`. |

## Sources

- [Home Assistant: Google Photos integration](https://www.home-assistant.io/integrations/google_photos/)
- [Home Assistant: Improv via BLE integration](https://www.home-assistant.io/integrations/improv_ble/)
- [Google Photos Picker API: Get started](https://developers.google.com/photos/picker/guides/get-started-picker)
- [Google Photos Picker API: List and retrieve media items](https://developers.google.com/photos/picker/guides/media-items)
- [Google Photos Picker API: Sessions resource](https://developers.google.com/photos/picker/reference/rest/v1/sessions)
- [ESP-IDF: ESP32-P4 JPEG peripheral](https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/peripherals/jpeg.html)
- [espressif/esp-hosted-mcu: ESP32-P4 Function EV board notes](https://github.com/espressif/esp-hosted-mcu/blob/main/docs/esp32_p4_function_ev_board.md)
- Vendor board bundle under `/source/JC8012P4A1C_I_W_Y/` (BSP headers and IDF 5.5.4 demos) — local, git-ignored

---

## R12. SD card bring-up: LDO power, slot 0, and exFAT

**Decision**: mount the card through the vendored BSP's `bsp_sdcard_mount()`, with
`CONFIG_BSP_SD_FORMAT_ON_MOUNT_FAIL=y` and FATFS long filenames enabled.

**Verified on hardware 2026-08-25**: `SD card mounted at /sdcard (60350 MB)` — the 64 GB card
partitioned, formatted to FAT32, and mounted.

Three facts about this board, none of which are guessable from the schematic alone. All came from
the manufacturer's own `esp_brookesia_phone` demo sources under `source/`:

1. **The SD rail is powered by an on-chip LDO on channel 4.** It must be brought up with
   `sd_pwr_ctrl_new_on_chip_ldo()` before any transaction. Skip it and the card never answers —
   indistinguishable from an empty slot, which is precisely the state this feature must report
   accurately (FR-030).
2. **The card is on SDMMC slot 0, routed through the IO MUX**, so its pins are fixed in silicon.
   Driving it through the GPIO matrix with an explicit pin list (`SdMmcHostDriver::new_4bits`)
   targets the wrong mechanism entirely.
3. **A stock 64 GB SDXC card ships exFAT, which ESP-IDF 5.5.3 cannot mount** — there is no
   `CONFIG_FATFS_*EXFAT*` symbol in the tree. FatFs returns error 13 (`NO_FILESYSTEM`), which reads
   like a dead card but is not. Format-on-mount-failure turns this into a self-healing path for any
   fresh card. **This erases the card, and was approved by the owner** on the grounds that it is a
   dedicated photo cache.

**Two dead ends, both of which compiled:**

- Hand-building `sdmmc_host_t` from the raw bindings crashes at `MEPC: 0x00000000`. The struct is
  largely function pointers that ESP-IDF fills via the `SDMMC_HOST_DEFAULT()` **macro**, and bindgen
  does not surface macros. It links cleanly and jumps to null on the first transaction.
- The BSP copy under `common_components/` **declares and calls `bsp_sdcard_get_sdmmc_host` and
  `bsp_sdcard_sdmmc_get_slot` but never defines them** — the vendor's SD path does not link as
  shipped. The complete definitions exist only in the `esp_brookesia_phone` copy of the same file,
  and are now carried as a marked local addition in the vendored component.

**Methodological note, and a direct echo of R11**: the owner's suggestion to check `./source` is what
resolved this. Two rounds of plausible-looking Rust had already failed. **When this board behaves
unexpectedly, read the vendor's demo sources before reasoning from first principles** — they encode
board facts that no amount of API-level inference will recover.

**Also corrected**: the BSP guards its long-filename warning on `CONFIG_FATFS_LONG_FILENAMES`, which
is a Kconfig *choice* rather than a config symbol. The test can never be satisfied, so the warning
fires even with long filenames enabled. Repointed at `CONFIG_FATFS_LFN_NONE`.

---

## R13. Owner-supplied photos on the SD card

**Decision**: the card carries two folders. `ha/` is the frame's own cache, ours to evict and clear.
`media/` belongs to the owner, is never written or deleted by the frame, and contains a
plain-language note explaining itself. **While `media/` holds photos the frame shows those and
nothing else**, ignoring Home Assistant entirely.

**Rationale**: it makes the frame a complete product on its own. Copy photos onto the card, plug it
in, switch it on -- no Wi-Fi, no adoption, no Home Assistant account. That matters for a gift: the
device works the moment it is unwrapped, and keeps working if the network it was set up on ever goes
away for good.

**Alternatives considered**: mixing local and Home Assistant photos into one rotation, and treating
`media/` as an offline fallback used only when Home Assistant is unreachable. Both were rejected by
the owner in favour of outright precedence, which is the only one of the three that is explainable
in two sentences on a card someone reads once.

**Consequence worth stating plainly**: dropping a single photo into `media/` silently switches off
Home Assistant curation for that frame. This is mitigated, not eliminated -- the frame reports
`photo_source` in its health message, the coordinator exposes `running_from_sd_card` and a
`local_photos_notice` explaining how to hand control back, and the note on the card says the same
thing. An owner who forgets will still be briefly puzzled.

**Fitting**: photos here have not been through `renderer.py`, so the frame fits them itself
(`frame_ui::fit`). Crop when the photo is within 25% of the panel's aspect -- the same tolerance the
Home Assistant renderer uses, so both paths make the same call about the same photo -- and otherwise
scale to fit and centre on black. **This diverges deliberately from Home Assistant's blurred
backdrop**: a 1280x800 gaussian blur is expensive on a 400 MHz core, black costs nothing, and the UI
is already true black because anything lighter shows the panel's mura (R11).

**Formats**: JPEG and PNG. **Not HEIC**, which is what an iPhone produces by default and therefore
the single most likely thing an owner will copy across. There is no HEIC decoder in the firmware;
unreadable files are counted and reported (`"200 file(s) on card, none readable"`) so a folder of
HEIC is explicable rather than mysterious, and the note on the card gives the export steps.

**Scanned once at boot**, not watched. Swapping the card is a power cycle, which is what the note
tells the owner to do anyway, and it keeps the frame off the filesystem during normal running.

**Testing note**: this logic lives in `frame-ui` rather than `frame-firmware` specifically so it can
be tested. `frame-firmware` cannot compile for the host -- it pulls `esp_idf_svc` -- so anything put
there is unreachable by `make test-host` and would go unverified.
