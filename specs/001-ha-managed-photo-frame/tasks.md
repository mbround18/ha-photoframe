---

description: "Task list for Home Assistant-Managed Digital Photo Frame"
---

# Tasks: Home Assistant-Managed Digital Photo Frame

**Input**: Design documents from `/specs/001-ha-managed-photo-frame/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

**Tests**: Test tasks are included. They are not optional here — Constitution Principle IX requires
`hassfest`-clean, tested integration code, the provider contract mandates a conformance suite and an
architecture test, and Principle VIII's "no developer chrome" is only checkable by an exhaustive
display-state inventory.

**Organization**: Phases map one-to-one onto the plan's milestones M0-M8 and are labelled with the
user story each delivers, so any single story can be built and demonstrated on its own.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on incomplete work)
- **[Story]**: The user story this task serves (US1-US7)
- Every task names the exact file it touches

## Path Conventions

- Home Assistant integration: `custom_components/photoframe_bridge/` (repository root, after T004)
- Integration tests: `tests/` (repository root)
- Firmware crates: `packages/frame-*/`
- Feature docs: `specs/001-ha-managed-photo-frame/`

---

## Phase 1 (M0): Setup — Groundwork

**Goal**: A repository that builds from a fresh checkout, ships where HACS can find it, and no
longer carries the on-device Google OAuth this feature replaces.

**No story label** — shared infrastructure.

- [x] T001 Make the firmware build portable. The hard-coded `CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="/home/mbruno/development/photoframe/..."` could not simply become a relative path: ESP-IDF resolves it against its own generated project directory. The committed `sdkconfig.defaults` now omits the key entirely and the Makefile generates `target/sdkconfig.partition.generated` with the absolute path at build time, appended to `ESP_IDF_SDKCONFIG_DEFAULTS`
- [x] T002 [P] Add a `storage`/`nvs` review to `partitions_16mb.csv`, confirming the `nvs` partition is large enough for the Wi-Fi credential, frame token, controller binding, and presentation settings from [data-model.md](./data-model.md)
- [x] T003 [P] Add `/source`, `rustc-ice-*.txt`, and `.claude/settings.local.json` to `.gitignore` and confirm `git check-ignore` passes for each
- [x] T004 Move the Home Assistant component from `packages/frame-ha-bridge/homeassistant/custom_components/photoframe_bridge/` to `custom_components/photoframe_bridge/` at the repository root, preserving git history with `git mv` (required by HACS — see [research.md](./research.md) R7)
- [x] T005 Create `hacs.json` at the repository root declaring `name`, `content_in_root: false`, `homeassistant` minimum version, and `render_readme`
- [x] T006 Update `custom_components/photoframe_bridge/manifest.json`: real `documentation` and `issue_tracker` URLs, `iot_class: local_push`, `integration_type: device`, and a `version` matching the release tag. Keep `config_flow: false` and pin `websockets>=13,<14` for now — `hassfest` fails on `config_flow: true` without a `config_flow.py`, and the current `controller.py` imports `websockets.legacy.server`, which websockets 14 removed. **T067 flips `config_flow` to true; T026 drops the `websockets` requirement entirely.**
- [x] T007 Remove the now-obsolete `package` target and `HA_COMPONENT_SOURCE` variables from `Makefile`, since HACS installs from the repo root instead of a tarball
- [x] T008 [P] Delete `packages/frame-api/src/oauth.rs` and `packages/frame-api/tests/google_photos.rs`, and remove their module declarations from `packages/frame-api/src/lib.rs` (Principle II)
- [x] T009 [P] Delete the Google OAuth routes, device-code polling, and consent templates from `packages/frame-captive-portal/src/lib.rs`, leaving only Wi-Fi setup
- [x] T010 [P] Remove Google/OAuth phases, fields, and UI strings from `packages/frame-core/src/state.rs` and `packages/frame-firmware/src/setup_state_store.rs`
- [x] T011 [P] Remove `GOOGLE_OAUTH_CLIENT_ID` / `GOOGLE_OAUTH_CLIENT_SECRET` from `.env.sample`, `scripts/bootstrap-env.sh`, `packages/frame-firmware/build.rs`, and the README's build instructions
- [x] T012 Verify no Google or OAuth symbols remain outside `custom_components/`: `grep -ri "oauth\|google" packages/ --include=*.rs` returns nothing
- [x] T012a Add `packages/frame-core/tests/no_third_party_credentials.rs`: an architecture test that fails the build if any credential-shaped identifier (`oauth`, `refresh_token`, `client_secret`, `api_key`, `access_token`) appears anywhere in `packages/`, allowing only `wifi_psk` and `frame_token`. This makes Constitution Principle II mechanically enforced rather than merely asserted, as Principles III and VIII already are
- [x] T013 Create `.github/workflows/ci.yaml` running `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test --workspace --target x86_64-unknown-linux-gnu`
- [x] T014 [P] Add a `hassfest` job to `.github/workflows/ci.yaml` using `home-assistant/actions/hassfest`
- [x] T015 [P] Add a `HACS` validation job to `.github/workflows/ci.yaml` using `hacs/action` with `category: integration`
- [x] T016 [P] Add `pytest` + `pytest-homeassistant-custom-component` dev dependencies to `pyproject.toml` and a `tests/conftest.py` enabling custom integrations
- [x] T017 Update `README.md` to describe the new architecture: HA owns photo sourcing and credentials, the frame is a rendering client, and installation is via HACS
- [x] T017a [P] Pin the board's verified configuration in `sdkconfig.defaults`: `CONFIG_BSP_LCD_TYPE_1280_800=y`, the ESP-Hosted SDIO pin map, and `CONFIG_ESP_HOSTED_GPIO_SLAVE_RESET_SLAVE=54`, cross-checked against [Hardware-Reference.md](../../docs/Hardware-Reference.md) §4 and §8

- [x] T001a Remove the dead `[patch.crates-io] i-slint-core = { path = "vendor/i-slint-core" }` from the root `Cargo.toml`. `vendor/` had been deleted, so dependency resolution failed for the whole workspace and nothing built
- [x] T001b Fix the root cause the vendored slint copy was patching around: `packages/frame-ui/Cargo.toml` enabled slint's `libm` feature on the ESP-IDF target, but ESP-IDF is a **std** target. `libm` routes float maths through `num_traits::Euclid` while std's inherent `f32::rem_euclid` is still in scope, which fails to compile inside `i-slint-core`. Switching that target to the `std` feature removes the need for a vendored fork
- [x] T001c Make `frame-ha-bridge`'s Rust tests runnable without a system `libpython3-dev`: `make test-host` resolves a uv-managed CPython that ships `libpython` and exports `PYO3_PYTHON` / `LD_LIBRARY_PATH` (constitution: prefer `uv` over apt)

**Checkpoint**: CI is green on all four jobs; a fresh clone builds firmware; no OAuth code remains.

---

## Phase 2: Foundational — Protocol and Control Plane

**Goal**: The shared wire contract and the bidirectional control channel that US1, US2, US4, and US5
all sit on. **Blocking** — no user story phase can complete without this.

**No story label** — shared infrastructure.

- [ ] T018 Extend `packages/frame-core/src/control.rs` with the message types from [control-protocol.md](./contracts/control-protocol.md): `Hello`, `Claim`, `Enqueue`, `EnqueuedPhoto`, `Show`, `Settings`, `CacheReport`, `Showing`, and the new `DeviceCommand` variants (`next`, `previous`, `pause`, `resume`, `screen_on`, `screen_off`, `factory_reset`)
- [ ] T019 Add `protocol_version` negotiation to `packages/frame-core/src/control.rs`, with unknown message types ignored rather than erroring (forward-compatibility rule 3 of the control contract)
- [ ] T020 [P] Add round-trip serde tests for every new message type in `packages/frame-core/src/control.rs`, including unknown-field and unknown-type tolerance
- [ ] T021 Mirror the new message types in `custom_components/photoframe_bridge/protocol.py`, keeping it dependency-free (no `frame_ha_bridge` import) so HACS installs need no Rust
- [ ] T022 Expose the new types through the PyO3 bindings in `packages/frame-ha-bridge/src/lib.rs` and `packages/frame-ha-bridge/python/frame_ha_bridge/__init__.py`
- [ ] T023 Add `tests/test_protocol_parity.py` asserting `protocol.py` and the Rust definitions agree on message names, field names, and enum values — the guard for "one protocol, two languages, one source of truth"
- [ ] T024 Migrate `custom_components/photoframe_bridge/__init__.py` from `async_setup` + `CONFIG_SCHEMA` to config entries: `async_setup_entry`, `async_unload_entry`, `async_reload_entry`, and `async_remove_entry` (FR-044, FR-046, Principle IX)
- [ ] T025 Create `custom_components/photoframe_bridge/const.py` entries for the new storage keys, entity platforms, defaults from `PresentationSettings`, and service names
- [ ] T026 Rework `custom_components/photoframe_bridge/controller.py` into `control_server.py`: a config-entry-owned WebSocket server with `hello` authentication, a 5-second unauthenticated timeout, 30-second pings, and per-frame session state. Rebuild it on HA's bundled `aiohttp` and **delete the `websockets` entry from `manifest.json` requirements** — the current code imports `websockets.legacy.server`, which was removed in websockets 14 while the manifest permits `>=13,<15`, so it breaks at import on 14.x
- [ ] T027 Add `custom_components/photoframe_bridge/entity.py` with a `PhotoFrameEntity` base that wires the device registry entry, `frame_id` unique IDs, and availability driven by the control channel
- [ ] T028 Create `custom_components/photoframe_bridge/coordinator.py` with a `DataUpdateCoordinator` subclass owning per-frame runtime state (`PhotoPool`, cursor, connection state) as specified in [data-model.md](./data-model.md)
- [ ] T029 [P] Add `tests/test_control_server.py` covering `hello` authentication, rejection of a bad token, the unauthenticated timeout, and `correlation_id` echo
- [ ] T030 Create `packages/frame-firmware/src/control_client.rs`: WebSocket client, `hello` on connect, exponential backoff from 1 s to 60 s with jitter, and a message dispatch loop
- [ ] T031 [P] Add reconnect-behaviour tests for the backoff schedule in `packages/frame-firmware/src/control_client.rs` (host-testable — keep the backoff calculation free of ESP-IDF types)

**Checkpoint**: A frame can connect, authenticate, and exchange control messages with Home
Assistant; the protocol parity test passes.

---

## Phase 3 (M1): User Story 3 — The photos look right on the screen (P1)

**Goal**: A photo on the SD card renders correctly oriented, correctly proportioned, and transitions
smoothly — with no controller involved at all.

**Independent test**: Copy prepared JPEGs onto the SD card by hand, boot the frame, and confirm they
display correctly and cross-fade cleanly. No Home Assistant needed (quickstart V3).

### Firmware: storage and hardware render path

- [ ] T031a [US3] **Fix the panel driver.** The upstream BSP installs an **ILI9881C** for `CONFIG_BSP_LCD_TYPE_1280_800`, but this board has a **JD9365** — confirmed on hardware, where the panel does not answer (`ili9881c: ID1: 0x0, ID2: 0x0, ID3: 0x0`) even though the backlight lights. Add `esp_lcd_jd9365` and drive the panel with `JD9365_PANEL_BUS_DSI_2CH_CONFIG()` / `JD9365_800_1280_PANEL_60HZ_DPI_CONFIG()`, DPHY LDO channel 3 at 2500 mV, reset on GPIO27, as the vendor's `video_lcd_display` demo does. **Nothing visual can be validated until this lands** ([Hardware-Reference.md](../../docs/Hardware-Reference.md) §6)
- [ ] T032 [US3] Create `packages/frame-firmware/components/frame_photo_render/` as a C component with `CMakeLists.txt` and `idf_component.yml`, depending on `esp_driver_jpeg`, `esp_driver_ppa`, and `esp_lcd`
- [ ] T033 [US3] Implement hardware JPEG decode in `frame_photo_render.c`: `jpeg_decoder_get_info` to read dimensions, `jpeg_alloc_decoder_mem` for aligned buffers, decode to RGB565, honouring the 16-byte output padding documented in [research.md](./research.md) R4
- [ ] T034 [US3] Implement PPA scale/rotate/present in `frame_photo_render.c`, driving the panel from the decoded buffer without any CPU-side rotation (replacing the software rotate in `frame_embedded_ui.c`)
- [ ] T035 [US3] Implement a double-buffered cross-fade in `frame_photo_render.c`: decode the next photo into a second PSRAM framebuffer and blend, so transitions never show a blank gap (FR-025, SC-004)
- [ ] T036 [US3] Add a narrow C header `frame_photo_render.h` exposing `frame_photo_render_init`, `frame_photo_render_show_file`, `frame_photo_render_set_brightness`, and `frame_photo_render_screen_power`
- [ ] T037 [US3] Create `packages/frame-firmware/src/photo_pipeline.rs` as the safe Rust wrapper over the C shim, returning `Result` and never panicking on a decode failure (FR-029)
- [ ] T038 [US3] Create `packages/frame-firmware/src/sd_cache.rs`: acquire the `ESP_LDO_VO4` channel that powers the TF socket's VDD, then mount the card via `esp_vfs_fat_sdmmc_mount` in 4-bit mode (D0=39, D1=40, D2=41, D3=42, CLK=43, CMD=44) with `format_if_mount_failed`, rooted at `/sdcard/photoframe/` ([research.md](./research.md) R5, R9)
- [ ] T039 [US3] Implement graceful degradation in `sd_cache.rs`: a missing, full, or unreadable card falls back to a small in-memory ring rather than stopping the slideshow (FR-030)
- [ ] T040 [US3] Create `packages/frame-firmware/src/slideshow.rs` driving the rotation timer, advance/previous, and next-photo prefetch so a transition never waits on a decode (FR-024)
- [ ] T041 [US3] Separate the two render paths in `packages/frame-ui/src/display.rs` and `packages/frame-firmware/src/runtime.rs`: Slint owns first-run setup screens only, `photo_pipeline` owns everything after adoption ([research.md](./research.md) R4)
- [ ] T042 [US3] Add backlight control on **GPIO23** (`LCD_PWM`, which gates the MP3202 boost `EN`) to `frame_photo_render.c` for brightness and screen-off, verifying screen-off measurably reduces power draw (FR-034). **Not GPIO25** — that is a USB pin; see [Hardware-Reference.md](../../docs/Hardware-Reference.md) §7

### Home Assistant: photo preparation

- [ ] T043 [P] [US3] Create `custom_components/photoframe_bridge/renderer.py` with the Pillow pipeline from [research.md](./research.md) R8, running entirely in an executor (`async_add_executor_job`) — never on the event loop
- [ ] T044 [US3] Implement EXIF handling in `renderer.py`: `ImageOps.exif_transpose` then strip metadata, so photos arrive upright and carry no location data (FR-020, FR-043)
- [ ] T045 [US3] Implement the `fill` treatment in `renderer.py`: proportional cover-fit and centre crop for photos matching the panel's aspect ratio (FR-021)
- [ ] T046 [US3] Implement the `letterbox_blur` treatment in `renderer.py` for portrait-on-landscape: a blurred, darkened, zoomed copy of the photo as backdrop with the sharp full photo composited on top (FR-022)
- [ ] T047 [US3] Enforce baseline JPEG encoding in `renderer.py` (`progressive=False`, quality 85, `optimize=True`) — the P4 hardware decoder rejects progressive JPEG
- [ ] T048 [US3] Create `custom_components/photoframe_bridge/photo_store.py`: content-addressed `photo_id` from `sha256(item_id + source_id + geometry + pipeline_version)`, atomic writes, and LRU eviction
- [ ] T049 [US3] Create `custom_components/photoframe_bridge/http_view.py` registering an authenticated `HomeAssistantView` at `/api/photoframe_bridge/photo/{photo_id}`, validating the bearer frame token and returning the status codes in [control-protocol.md](./contracts/control-protocol.md)

### Tests

- [ ] T050 [P] [US3] Add `tests/test_renderer.py` asserting every output re-parses as **baseline** (not progressive) JPEG — the failure mode that would silently break on hardware
- [ ] T051 [P] [US3] Add orientation tests to `tests/test_renderer.py` covering all 8 EXIF orientation values
- [ ] T052 [P] [US3] Add geometry tests to `tests/test_renderer.py`: output is exactly the requested size, aspect ratio preserved, for landscape, portrait, square, oversized, and undersized inputs
- [ ] T053 [P] [US3] Add `tests/test_photo_store.py` covering content-addressing idempotence, `pipeline_version` cache invalidation, and LRU eviction order
- [ ] T054 [P] [US3] Add `tests/test_http_view.py` covering 200, 401 on a bad token, 404 on an evicted photo, and 503 while preparation is in flight
- [ ] T055 [US3] Assemble the awkward test-photo corpus from quickstart V3 (8 EXIF orientations, 50 MP, 200x150, CMYK, PNG+alpha, progressive JPEG, a video) under `tests/fixtures/photos/`

**Checkpoint**: Hand-placed photos display correctly and transition smoothly (quickstart V3). US3 is
demonstrable on its own.

---

## Phase 4 (M2, M3): User Story 1 — Adoption (P1)

**Goal**: Factory-fresh frame to adopted Home Assistant device, with no companion app.

**Independent test**: Erase NVS, power on, provision Wi-Fi, adopt from the discovery card
(quickstart V1).

### M2: The BLE decision gate

- [x] T056 [US1] **SPIKE — DECISION GATE**: bring up a connectable BLE GATT peripheral on the ESP32-P4 through the ESP32-C6. The shipped C6 firmware already advertises `HCI Over SDIO` and contains `slave_bt.c`, so this is enable-and-verify, not feasibility ([research.md](./research.md) R3, R9). Set `CONFIG_BT_ENABLED=y` plus the NimBLE host and ESP-Hosted BT transport; keep the SDIO pin map (D0-D3=14-17, CLK=18, CMD=19) and `CONFIG_ESP_HOSTED_GPIO_SLAVE_RESET_SLAVE=54`, which is correct on this board because P4 GPIO54 drives the C6 `EN` pin. Soak advertising for 30 minutes to check HCI-over-SDIO stability — no vendor demo enables BT, so this path is supported but unexercised. **Evidence**: serial log plus a phone-scanner screenshot
- [x] T057 [US1] Record the spike outcome in `specs/001-ha-managed-photo-frame/research.md` as a dated go/no-go, then take **exactly one** of T058 or T059-T061.
      **RESULT 2026-08-25: GO.** Verified on hardware — the frame advertises as `PhotoFrame-B566`,
      is visible from an independent Bluetooth adapter at -43 dBm, and accepts connections
      (`BLE connect: status=0`), with Wi-Fi up simultaneously. See [research.md](./research.md) R10.
      **Build Branch A (T058). Do not build Branch B.**

### Branch A — spike passed (preferred)

- [ ] T058 [US1] Implement the Improv Wi-Fi BLE GATT service, replacing the spike component `packages/frame-firmware/components/frame_ble_spike/` (which already proves NimBLE-over-ESP-Hosted works and can be extended in place rather than rewritten). Keep the spike's handling of `ESP_ERR_NOT_SUPPORTED` from `esp_hosted_bt_controller_init` — this board's ESP-Hosted slave firmware is 2.1.0 and predates that RPC ([research.md](./research.md) R10). Service in `packages/frame-net/src/improv_ble.rs`: service UUID `00467768-6228-2272-4663-277478268000`, the five characteristics, and the `authorized → provisioning → provisioned` state machine from [discovery.md](./contracts/discovery.md), returning `unable_to_connect` on a bad password so the user can retry immediately (FR-002)

### Branch B — NOT NEEDED (the T056 spike passed; these are cancelled)

- [~] T059 [US1] ~~(cancelled - spike passed)~~ Strip `packages/frame-captive-portal/src/lib.rs` to a single Wi-Fi-join page with a captive-portal redirect, with no other routes
- [~] T060 [US1] ~~(cancelled - spike passed)~~ Raise a `PhotoFrame-XXXX` open SoftAP in `packages/frame-net/src/provisioning.rs`, torn down permanently once Wi-Fi is joined and never re-raised unless the frame is reset
- [~] T061 [US1] ~~(cancelled - spike passed)~~ Add retry-without-reset handling to the fallback page so a wrong password returns the user to network selection (FR-002)

### M3: Discovery and adoption (both branches)

- [ ] T062 [US1] Stop BLE advertising (or AP broadcast) the moment `adopted` becomes true in `packages/frame-firmware/src/runtime.rs` — no standing radio surface on an adopted frame ([discovery.md](./contracts/discovery.md) security property 2)
- [ ] T063 [US1] Implement mDNS announcement of `_photoframe._tcp.local.` in `packages/frame-net/src/provisioning.rs` with a stable instance name and the `frame_id`, `fw`, `panel`, `adopted`, `proto` TXT records
- [ ] T064 [US1] Derive a stable `frame_id` from the P4 eFuse MAC in `packages/frame-firmware/src/ownership_store.rs`, surviving reboots, resets, and network changes (FR-005)
- [ ] T065 [US1] Persist the controller binding and frame token to NVS in `packages/frame-firmware/src/ownership_store.rs`, and refuse a claim from a different controller while adopted (FR-006)
- [ ] T066 [US1] Implement mDNS re-resolution in `packages/frame-firmware/src/control_client.rs` so a Home Assistant that changed address is rediscovered rather than requiring re-adoption
- [ ] T067 [US1] Create `custom_components/photoframe_bridge/config_flow.py` and set `"config_flow": true` plus `"zeroconf": ["_photoframe._tcp.local."]` in `manifest.json`, with `async_step_zeroconf`: parse TXT, set `unique_id = frame_id`, and `_abort_if_unique_id_configured(updates={CONF_HOST: host})` so a moved frame updates in place
- [ ] T068 [US1] Add `async_step_confirm` to `config_flow.py` showing the frame's name and panel geometry, taking the owner's chosen name, and minting the `frame_token`
- [ ] T069 [US1] Abort discovery with `already_adopted` in `config_flow.py` when the TXT record says `adopted=1` and the binding is not ours (FR-006)
- [ ] T070 [US1] Add `async_step_user` manual host entry to `config_flow.py` for networks where mDNS does not cross subnets
- [ ] T071 [US1] Declare `"zeroconf": ["_photoframe._tcp.local."]` (and `"bluetooth"` for Branch A) in `custom_components/photoframe_bridge/manifest.json`
- [ ] T072 [US1] Implement the claim handshake in `control_server.py` and `control_client.rs`: `hello` with an empty token, `claim` response carrying the minted token and settings, persisted to NVS ([discovery.md](./contracts/discovery.md))
- [ ] T073 [US1] Design the first-run screens in `packages/frame-ui/ui/main.slint`: plain language, no IP addresses, no error codes, no logs — the only screens permitted to show setup content
- [ ] T074 [P] [US1] Add `tests/test_config_flow.py` covering zeroconf discovery, duplicate-frame abort, host update on rediscovery, the already-adopted abort, and manual entry
- [ ] T075 [P] [US1] Add `tests/test_claim.py` covering successful claim, refusal of a second controller, and token persistence

**Checkpoint**: Factory-fresh to adopted, timed against SC-001 (10 min) and SC-002 (2 min).

---

## Phase 5 (M5, M6): User Story 2 — Choosing which photos appear (P1)

**Goal**: The owner picks a source and a selection in Home Assistant; those photos appear.

**Independent test**: On an adopted frame, configure a source and selection and confirm the right
photos appear within 60 s (quickstart V2).

### The provider seam

- [ ] T076 [US2] Create `custom_components/photoframe_bridge/providers/__init__.py` with the `PhotoProvider` ABC, the `Capabilities` dataclass, the `register_provider` registry, and the exception hierarchy from [photo-provider.md](./contracts/photo-provider.md)
- [ ] T077 [US2] Define `Collection`, `PhotoRef`, and `Selection` in `providers/__init__.py` per [data-model.md](./data-model.md)
- [ ] T078 [US2] Add provider-contributed config-flow hooks (`async_config_steps`, `async_selection_steps`) to the ABC so `config_flow.py` never branches on provider identity
- [ ] T079 [P] [US2] Implement `providers/sample.py`: a bundled photo set, no auth, no collections — the seam proof and the content an adopted-but-unconfigured frame shows so it is never blank
- [ ] T080 [P] [US2] Implement `providers/media_source.py` over Home Assistant's `media_source` helpers, browsing to collections and yielding items lazily; `supports_live_collections=True`
- [ ] T081 [US2] Implement `providers/google_photos_picker.py`: `application_credentials` wiring, the `photospicker.mediaitems.readonly` scope, and `sessions.create`
- [ ] T082 [US2] Implement the picker wait loop in `providers/google_photos_picker.py`, polling `sessions.get` until `mediaItemsSet` and honouring the server-supplied `pollingConfig` rather than a fixed interval
- [ ] T083 [US2] Implement `mediaItems.list` with pagination in `providers/google_photos_picker.py`, presenting the picked set as one synthetic collection so the UI stays consistent (FR-009)
- [ ] T084 [US2] Implement `async_fetch_bytes` in `providers/google_photos_picker.py`: request `=w2560-h1600` (2x the panel, so crops stay sharp) with an `Authorization: Bearer` header, re-listing when a `baseUrl` has passed its 60-minute life
- [ ] T085 [US2] Track the session `expireTime` in `providers/google_photos_picker.py` and set `Selection.expires_at`; declare `supports_live_collections=False`, `selection_expires=True` ([research.md](./research.md) R2)
- [ ] T086 [US2] Create `custom_components/photoframe_bridge/application_credentials.py` for the Google OAuth authorization server
- [ ] T087 [US2] Add the provider-selection and selection steps to `custom_components/photoframe_bridge/config_flow.py` as an options flow, delegating entirely to the provider hooks from T078
- [ ] T088 [US2] Surface frozen-selection and expiry semantics in the options flow UI, stating plainly that a picker selection is fixed and how to revise it (FR-014, FR-014a, US2 scenario 8)
- [ ] T089 [US2] Add a re-pick repair flow (`async_step_reauth` / `async_step_reconfigure`) to `config_flow.py`, raising `ConfigEntryAuthFailed` on `NeedsReauth` and `SelectionExpired` (FR-014b, FR-038)
- [ ] T090 [US2] Report an empty resolved pool at selection time in the options flow rather than letting it surface later as a blank frame (edge case)

### Delivery

- [ ] T091 [US2] Implement pool resolution in `coordinator.py`: resolve the selection to a `PhotoPool`, deduplicate across collections, filter non-image media (FR-018), and cap memory by consuming provider iterators lazily
- [ ] T092 [US2] Implement play ordering in `coordinator.py` as a seeded permutation over indices, so no photo repeats until the pool is exhausted and the order survives a reload (FR-015)
- [ ] T093 [US2] Implement scheduled pool refresh in `coordinator.py`, scheduled **only** when `capabilities.supports_live_collections` (FR-014, [research.md](./research.md) R2)
- [ ] T094 [US2] Implement the `enqueue` push in `coordinator.py`: prepare upcoming photos, send `photo_id` + HA-relative path + `sha256` + evictions, honouring the frame's `cache_report` so photos it already holds are not re-sent
- [ ] T095 [US2] Implement `enqueue` and `show` handling in `packages/frame-firmware/src/control_client.rs`: fetch over HTTP, verify the `sha256`, write to a temp file, and `rename` atomically so a power loss cannot leave a half photo (edge case)
- [ ] T096 [US2] Implement prepared-photo fetching in `packages/frame-api/src/client.rs` with the bearer frame token and the status-code handling from [control-protocol.md](./contracts/control-protocol.md)
- [ ] T097 [US2] Apply a selection change without restart or re-adoption in `coordinator.py`, clearing the old queue and pushing the new pool (FR-013, SC-008)

### Tests

- [ ] T098 [P] [US2] Add `tests/providers/test_conformance.py` — the shared suite parametrized over the registry, covering all six conformance rules in [photo-provider.md](./contracts/photo-provider.md)
- [ ] T099 [P] [US2] Add `tests/test_provider_isolation.py`: fail the build if any provider key or provider class name appears outside `providers/` — the mechanical enforcement of Principle III
- [ ] T100 [P] [US2] Add `tests/providers/test_google_photos_picker.py` with a mocked Picker API covering session creation, the poll loop, pagination, `baseUrl` expiry and re-listing, and session expiry
- [ ] T101 [P] [US2] Add `tests/test_coordinator.py` covering deduplication, the no-repeat-until-exhausted invariant, refresh scheduling by capability, and cache-aware enqueue

**Checkpoint**: Photos from a local media source and from Google Photos appear on the frame. With
Phases 3-5 done, the **MVP is complete** — all three P1 stories ship.

---

## Phase 6 (M7): User Story 4 — It keeps working when things go wrong (P2)

**Independent test**: Restart Home Assistant, pull the network for 30 minutes, cut power — the
slideshow never stops (quickstart V4).

- [ ] T102 [US4] Implement the SD cache index at `/sdcard/photoframe/index.json` in `packages/frame-firmware/src/sd_cache.rs` with the fields and invariants from [data-model.md](./data-model.md)
- [ ] T103 [US4] Implement LRU eviction by `last_shown_at` in `sd_cache.rs`, never evicting the current or next-prefetched photo, capped by count rather than bytes ([research.md](./research.md) R5)
- [ ] T104 [US4] Implement integrity verification in `sd_cache.rs`: check size and JPEG SOI/EOI markers on load and discard anything that fails, so a corrupt file is never displayed
- [ ] T105 [US4] Implement the cache-first boot path in `packages/frame-firmware/src/slideshow.rs`: start playing from SD before Wi-Fi association, let alone a controller connection (FR-027, SC-007)
- [ ] T106 [US4] Implement the `cache_only` mode transition in `slideshow.rs` when the control channel drops, cycling held photos indefinitely rather than going blank (FR-026, edge case)
- [ ] T107 [US4] Send `cache_report` on connect and on material cache change in `control_client.rs` so Home Assistant resumes without re-sending photos the frame already holds (SC-006)
- [ ] T108 [US4] Isolate provider failures in `coordinator.py`: `SourceUnavailable` marks source health and retries with backoff without touching the frame's queue (FR-017, US4 scenario 5)
- [ ] T109 [P] [US4] Add `tests/test_resilience.py` covering controller restart mid-slideshow, provider outage isolation, and reconnect-and-resume
- [ ] T110 [US4] Run quickstart V4 on hardware end to end and capture one continuous video across all five steps as SC-005 evidence

**Checkpoint**: Quickstart V4 passes with recorded evidence.

---

## Phase 7 (M4): User Story 5 — Controlling the frame from Home Assistant (P2)

**Independent test**: Exercise every control manually and from an automation; each responds within
2 s (quickstart V5).

- [ ] T111 [P] [US5] Create `custom_components/photoframe_bridge/sensor.py`: connection state, currently-showing photo, cache fill, and source health (FR-031)
- [ ] T112 [P] [US5] Create `custom_components/photoframe_bridge/image.py` exposing the currently-displayed photo as an `ImageEntity` for dashboards
- [ ] T113 [P] [US5] Create `custom_components/photoframe_bridge/switch.py` for `screen_on` and `paused` (FR-032, FR-034)
- [ ] T114 [P] [US5] Create `custom_components/photoframe_bridge/number.py` for `rotation_interval_s`, `brightness`, `pool_refresh_interval_s`, and `cache_target_count` (FR-033)
- [ ] T115 [P] [US5] Create `custom_components/photoframe_bridge/select.py` for `order` and `transition`
- [ ] T116 [P] [US5] Create `custom_components/photoframe_bridge/button.py` for next, previous, refresh pool, and reboot
- [ ] T117 [US5] Register the `display_photo`, `send_command`, and a new `show_photo` service in `services.yaml` and `__init__.py`, each awaiting the frame's `correlation_id` echo so automations can sequence on them (FR-035, FR-036)
- [ ] T118 [US5] Implement settings persistence to NVS in `packages/frame-firmware/src/ownership_store.rs` so brightness, interval, and order survive a power cycle (FR-033)
- [ ] T119 [US5] Implement `next`, `previous`, `pause`, `resume`, `screen_on`, `screen_off` handling in `packages/frame-firmware/src/slideshow.rs`, acknowledging within the 2-second budget (SC-010)
- [ ] T120 [P] [US5] Add `tests/test_entities.py` covering every platform's state, availability, and command dispatch
- [ ] T121 [P] [US5] Add `tests/test_services.py` covering `show_photo` on a photo the frame does not hold — it must fetch first and not show a gap
- [ ] T121a [US5] Add `tests/test_multi_frame.py` covering two frames adopted in one Home Assistant: independent naming, independent selections, no collision on `photo_id` when panel geometries differ, isolated control-server sessions, and two frames sharing one selection without interfering (FR-007, edge cases)

**Checkpoint**: Quickstart V5 passes manually and from an automation.

---

## Phase 8: User Story 6 — Adding a new kind of photo source (P3)

**Independent test**: The `sample` provider was added by touching only `providers/` — verifiable
from that commit's diff alone (quickstart V6).

- [ ] T122 [US6] Verify SC-013 by inspecting the T079 commit: confirm it changed no file outside `custom_components/photoframe_bridge/providers/` and `strings.json`, and record the finding in the milestone evidence
- [ ] T123 [US6] Document the provider-authoring process in `docs/Adding-A-Photo-Source.md`: the ABC, capabilities, error contract, registration, and the conformance suite a new provider inherits
- [ ] T124 [US6] Handle collection-less sources uniformly in the options flow so they present a sensible selection experience rather than an empty list (US6 scenario 3)
- [ ] T125 [P] [US6] Add a per-source health sensor and repair issue so a misconfigured source reports against itself only (US6 scenario 4, FR-017)

**Checkpoint**: A new provider can be written from the doc alone, against the conformance suite.

---

## Phase 9: User Story 7 — Handing the frame on or starting over (P3)

**Independent test**: Adopt, reset, then dump NVS and the SD card — nothing personal remains
(quickstart V7).

- [ ] T126 [US7] Send `factory_reset` from `async_remove_entry` in `custom_components/photoframe_bridge/__init__.py` so removing the entry returns the frame to adoptable (FR-039)
- [ ] T127 [US7] Implement controller-triggered reset in `packages/frame-firmware/src/ownership_store.rs`: clear the token, binding, and SD cache, keep Wi-Fi, resume mDNS with `adopted=0` and BLE advertising
- [ ] T128 [US7] Implement the on-device reset gesture as a **press-and-hold of the BOOT button on GPIO35** (active low, 10K pullup) for 10 s, followed by an explicit on-screen confirmation, in `packages/frame-firmware/src/runtime.rs` (FR-040, FR-041). Chosen over a touch gesture because this board's touch controller is a GSL3680 that the stock BSP cannot drive, and a physical button cannot be triggered by dusting the screen ([research.md](./research.md) R9)
- [ ] T129 [US7] Implement full erase on device reset: NVS Wi-Fi credential, token, binding, and every file under `/sdcard/photoframe/` (FR-042)
- [ ] T130 [US7] Confirm clean integration removal leaves no orphaned device, entity, credential, or prepared-photo cache (FR-046)
- [ ] T131 [P] [US7] Add `tests/test_removal.py` covering entry removal, credential cleanup, and cache teardown
- [ ] T132 [US7] Run quickstart V7 on hardware: dump NVS and mount the SD card on a PC and assert no token, PSK, controller reference, or photo bytes remain (SC-014)

**Checkpoint**: Quickstart V7 passes with dumped evidence.

---

## Phase 10 (M8): Polish & Cross-Cutting Concerns

**No story label** — cross-cutting.

- [ ] T133 Produce the exhaustive display-state inventory required by SC-012: enumerate every reachable screen of an **adopted** frame (normal playback, controller down, network down, SD failed, decode failed, empty pool, screen off, mid-reset) and confirm none shows an address, identifier, stack trace, error code, or version string (Principle VIII)
- [ ] T134 Route all diagnostics to the serial console and Home Assistant entities only, removing any remaining on-panel status text from `packages/frame-firmware/components/frame_embedded_ui/frame_embedded_ui.c` (FR-037)
- [ ] T134a [P] Drive the on-board **WS2812 RGB LED on GPIO26** as the frame's non-visual status channel: setup progress, provisioning result, controller-connection loss, and reset confirmation. This lets the frame communicate state while keeping the panel free of text (Principle VIII, FR-037; [research.md](./research.md) R9)
- [ ] T135 [P] Create `custom_components/photoframe_bridge/diagnostics.py` with redacted config-entry diagnostics — tokens and credentials must never appear in a downloaded diagnostics file
- [ ] T136 [P] Complete `custom_components/photoframe_bridge/strings.json` and `translations/en.json` so every user-visible string is translatable, with no hard-coded English in Python (FR-045, Principle IX)
- [ ] T137 [P] Write plain-language repair issues for the failure states an owner must act on: unavailable selection, expired credential, expired picker session (FR-038)
- [ ] T138 [P] Audit `custom_components/photoframe_bridge/` for blocking I/O on the event loop — every Pillow call, file write, and provider HTTP call must be awaited or in an executor (Principle IX)
- [ ] T139 Run quickstart V9 against a 20,000-photo source and confirm browsing stays responsive and memory does not grow with pool size (SC-011)
- [ ] T140 Run the 7-day soak from quickstart V10, capturing serial heap high-water marks and confirming no reboot, memory growth trend, or visible fault (SC-016)
- [ ] T141 [P] Update `README.md` and `docs/Build-Your-Own.md` for HACS installation, adoption, and photo-source setup
- [ ] T141a [P] *(optional)* Vendor the `esp_lcd_touch_gsl3680` driver from `/source/.../common_components/` into `packages/frame-firmware/components/` if touch input is ever wanted. Not required by any current requirement — the reset gesture uses the GPIO35 button instead ([research.md](./research.md) R9)
- [ ] T142 [P] Rewrite `docs/TODO.md`, removing the superseded on-device OAuth and album-selection plans that this feature replaces
- [ ] T143 Tag a release and verify end-to-end HACS installation from the GitHub repository as a custom repository (FR-044, SC-015)

---

## Dependencies

```
Phase 1 (M0 Setup)
    │
    v
Phase 2 (Foundational: protocol + control channel)  ◄── BLOCKING for US1, US2, US4, US5
    │
    ├──> Phase 3 (US3 render) ──────────┐   [independent of the control channel; can start early]
    │                                    │
    ├──> Phase 4 (US1 adoption)          │
    │        │ T056 SPIKE = decision gate│
    │        └─> Branch A (T058) XOR Branch B (T059-T061)
    │                    │               │
    │                    v               v
    │            Phase 5 (US2 sources & delivery)   ◄── needs US1 + US3
    │                    │
    │                    ├──> Phase 6 (US4 resilience)
    │                    ├──> Phase 7 (US5 controls)
    │                    ├──> Phase 8 (US6 extensibility)
    │                    └──> Phase 9 (US7 reset)
    │                              │
    └──────────────────────────────┴──> Phase 10 (M8 Polish)
```

**Story dependencies**:

| Story | Depends on | Why |
|---|---|---|
| US3 (render) | Phase 1 only | Testable with hand-placed SD photos, no controller |
| US1 (adoption) | Phase 2 | Claim rides the control channel |
| US2 (sources) | US1, US3 | Needs an adopted frame that can display |
| US4 (resilience) | US2 | Needs a real delivery path to interrupt |
| US5 (controls) | US2 | Controls act on a running slideshow |
| US6 (extensibility) | US2 | Proven against the shipped provider seam |
| US7 (reset) | US1 | Resets an adoption |

**Critical path**: T001 → T004 → T018 → T026/T030 → T056 (gate) → T072 → T076 → T094 → MVP.

**T056 is the schedule risk.** It gates Phase 4 and can only be resolved on hardware. Run it as
early as Phase 1 allows — it is independent of Phases 2 and 3 and needs only a flashable board.

---

## Parallel Opportunities

**Phase 1**: T002, T003, T008-T011, T014-T016 are independent files.

**Phase 2**: T020, T029, T031 are independent test files.

**Phase 3**: the firmware track (T032-T042) and the Home Assistant track (T043-T049) touch disjoint
trees and can run concurrently. All of T050-T054 are independent test files.

**Phase 4**: T074 and T075 run in parallel; T063-T066 (firmware) and T067-T071 (integration) are
concurrent tracks once the T056 gate resolves.

**Phase 5**: T079 and T080 are independent providers. T098-T101 are independent test files. The
Google provider (T081-T086) and the delivery path (T091-T097) are concurrent tracks.

**Phase 7**: T111-T116 are six independent entity platform files — the widest parallel block in the
plan.

**Phase 10**: T135-T138 and T141-T142 are independent.

---

## Implementation Strategy

**MVP = Phases 1, 2, 3, 4, 5** — all three P1 stories. That is a frame you can adopt, point at an
album, and hang on a wall. Everything after is what makes it an appliance rather than a
demonstration.

**Suggested order**:

1. **Phase 1** — unblocks everything and fixes the build.
2. **T056 spike immediately after**, in parallel with Phase 2. It is the only task that can force a
   design change, so learn the answer while there is still cheap time to absorb it.
3. **Phase 3** alongside Phase 2 — the render path is independently testable and is the highest-risk
   *quality* work (SC-003, SC-004 are the criteria most likely to need iteration).
4. **Phase 4, then Phase 5** — the MVP.
5. **Phase 6** before Phases 7-9: resilience is what separates a gift from a support burden, and
   later phases are easier to test against a frame that does not fall over.
6. **Phase 10** last, with T133 and T140 as the real gates. The soak takes a week of wall-clock time,
   so start it while polishing the rest.

**Do not skip**: T023 (protocol parity), T050 (baseline JPEG), and T099 (provider isolation). Each
guards a failure that is silent at build time and expensive on hardware.
