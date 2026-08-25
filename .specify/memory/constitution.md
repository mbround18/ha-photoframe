<!--
Sync Impact Report
Version change: (none) → 1.0.0
Modified principles: n/a (initial ratification)
Added sections: Core Principles (I-IX), Hardware & Toolchain Constraints, Development Workflow, Governance
Removed sections: none
Templates requiring updates:
  - .specify/templates/plan-template.md ✅ reads constitution at runtime, no change needed
  - .specify/templates/spec-template.md ✅ no change needed
  - .specify/templates/tasks-template.md ✅ no change needed
Follow-up TODOs: none
-->

# ha-photoframe Constitution

## Core Principles

### I. std Rust on ESP-IDF (NON-NEGOTIABLE)

Firmware MUST be written as std Rust targeting `esp32p4` via `esp-idf-svc` / `esp-idf-hal` /
`esp-idf-sys`, not bare-metal `no_std` (`esp-hal` / `esp-wifi`). The ESP32-P4 has no native radio;
Wi-Fi and BLE are reachable only through the onboard ESP32-C6 co-processor over ESP-Hosted (SDIO),
and the MIPI-DSI display, capacitive touch, and SDMMC drivers for this panel are mature only in
ESP-IDF. Re-evaluate only via a formal amendment, never ad hoc.

### II. Home Assistant Is The Only Control Plane

The frame MUST NOT hold credentials for any third-party photo service. It stores exactly two
secrets: the Wi-Fi credential and a Home Assistant-issued frame token. All photo-service OAuth
tokens, refresh tokens, and API keys live in Home Assistant's config-entry storage and never
traverse the device. Rationale: this device sits unattended in a living room and can be physically
carried off; a stolen frame MUST NOT be a stolen Google account. A frame that is factory-reset or
un-adopted MUST lose all access with no action required at the photo provider.

### III. Pluggable Provider Model

Photo sourcing in the integration MUST sit behind a single stable `PhotoProvider` interface
(enumerate collections → list items → resolve one item to fetchable bytes + metadata). Concrete
providers (Home Assistant media sources, direct Google Photos Picker, and later S3, Immich,
Nextcloud, local folders) MUST be additive: adding a provider MUST NOT require changes to the
coordinator, the render pipeline, the device protocol, or the frame firmware. No provider-specific
type, field, or branch may appear outside its own provider module. Rationale: the Google Photos
API surface has already broken once (Library API scopes withdrawn 2025-03-31) and will break
again; provider churn must stay contained to one file.

### IV. Local-Network-Only Runtime

After adoption, all traffic between the frame and Home Assistant MUST stay on the local network:
mDNS discovery, the control channel, and photo fetches. The frame MUST NOT contact any cloud
service directly, including the photo provider's. Home Assistant reaching a provider's cloud API
on the user's behalf is expected and in scope; the frame doing so is not.

### V. Zero-Configuration Adoption, No Companion App

A factory-fresh frame MUST become fully operational with no companion app, no build-time
hardcoded credentials, and no cloud-hosted setup service. The path is: the frame advertises Improv
Wi-Fi over BLE, the user provisions Wi-Fi from Home Assistant or a browser-based Improv page, the
frame announces itself over mDNS, and Home Assistant surfaces a discovery card that adopts it in a
config flow. Every step MUST be independently re-runnable after a failure without a factory reset.

### VI. The Frame Renders, Home Assistant Works

All image decode, orientation correction, crop, resize, and re-encode work MUST happen in Home
Assistant. The frame MUST receive photos already encoded to its exact panel geometry and MUST NOT
implement a general-purpose image pipeline. Rationale: a 400MHz RISC-V core decoding multi-megapixel
originals cannot produce smooth transitions, and every format quirk becomes a firmware bug on
hardware that is hard to debug.

### VII. The Frame Keeps Showing Photos

The SD card cache is a first-class durability boundary, not an optimization. The frame MUST hold a
local cache of pre-rendered photos sufficient to continue the slideshow through a Home Assistant
restart, a network outage, or a provider API failure, and MUST resume cleanly when the control
plane returns. Cache loss MUST degrade to "fetch again", never to a blank or error screen.

### VIII. Consumer-Grade On-Device Experience (NON-NEGOTIABLE)

Once a frame is adopted, its screen MUST show photos and nothing else. No status text, no IP
addresses, no error codes, no correlation IDs, no progress logs, no developer chrome. Failure
states MUST be expressed as either the last good photo remaining on screen or a single plain,
human-readable sentence a non-technical person can act on. Diagnostics belong on the serial console
and in Home Assistant entities, never on the panel. Rationale: this is a gift, not a dev board.

### IX. Home Assistant Integration Quality Standards

The `photoframe_bridge` integration MUST be installable through HACS as a custom repository and
MUST satisfy Home Assistant's integration requirements: a UI config flow (no YAML-only setup),
`hassfest` and `hacs/action` clean in CI, a `DataUpdateCoordinator` for polled state, config-entry
setup/unload/reload, `strings.json` translations for every user-visible string, typed async code,
and no blocking I/O on the event loop. Anything that blocks — image processing, file writes,
provider HTTP calls — MUST run in an executor or a dedicated task.

## Hardware & Toolchain Constraints

Target hardware is the Shenzhen Jingcai JC8012P4A1C_I_W: ESP32-P4 dual-core RISC-V @ 400MHz,
768KB L2MEM, 32MB PSRAM, 16MB flash (W25Q128), ESP32-C6 co-processor for Wi-Fi/BLE over SDIO,
10.1" 800x1280 IPS panel on MIPI-DSI behind a JD9365 driver, capacitive touch over I2C, SDMMC TF
card slot (64GB card fitted), ES8311 audio codec, RX8025T RTC. Vendor documentation lives under
`/source` and is deliberately git-ignored; it is reference material, not redistributable.

Toolchain and SDK versions (Rust channel via `rust-toolchain.toml`, ESP-IDF version, `espflash` /
`ldproxy`, `Cargo.lock`, `sdkconfig.defaults`, `partitions_16mb.csv`) MUST be pinned in
version-controlled files. Any change to a pinned version MUST be a deliberate, tested commit, never
an incidental side effect of a fresh environment picking up "latest".

Crates MUST remain host-testable where they can be. `frame-core`, `frame-api`, and `frame-ha-bridge`
MUST compile and run their tests on `x86_64-unknown-linux-gnu` with no ESP-IDF dependency; only
`frame-firmware`, `frame-net`, and the hardware-facing half of `frame-ui` may require the embedded
target. Protocol types MUST have a single source of truth shared between the Rust device side and
the Python Home Assistant side, with round-trip tests on both.

## Development Workflow

Each milestone MUST have its own spec (`/speckit-specify`) and plan (`/speckit-plan`) before
implementation begins. Features MUST be built and verified in dependency order, each confirmed
working before the next is layered on:

1. Wi-Fi bring-up and Improv BLE provisioning
2. mDNS announcement and Home Assistant discovery + adoption config flow
3. Control channel and device entities
4. Provider interface with the first concrete provider and the render pipeline
5. SD cache and slideshow presentation
6. Consumer polish, ownership/reset, and diagnostics

A milestone is not "done" until its evidence is captured and reviewed: serial-log output for
firmware milestones, on-screen photographs for display milestones, and passing `hassfest` plus
integration tests for Home Assistant milestones. Compiling and flashing is not evidence.

## Governance

This constitution supersedes ad hoc technical preferences for this repository. Amendments require:
(1) a proposed diff to this file, (2) an explicit rationale, (3) a version bump per semantic
versioning (MAJOR for incompatible principle removal or redefinition, MINOR for new or materially
expanded principles, PATCH for clarifications), and (4) an updated Sync Impact Report comment at
the top of this file.

All specs and plans produced by spec-kit commands in this repository MUST be checked against these
principles before implementation starts. Any deviation MUST be justified in the plan's Complexity
Tracking section or rejected. Principles marked NON-NEGOTIABLE (I, II, VIII) MUST NOT be waived in
Complexity Tracking; they require a constitution amendment instead.

**Version**: 1.0.0 | **Ratified**: 2026-08-25 | **Last Amended**: 2026-08-25
