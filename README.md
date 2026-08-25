# Home Assistant Photo Frame

A digital photo frame you set up once and then forget about. Home Assistant
chooses the photos, prepares them, and sends them to the frame; the frame's only
job is to show them beautifully — and to keep showing them when Home Assistant
is not there.

- **Firmware** — std Rust on ESP-IDF for an ESP32-P4 board with a 10.1"
  800x1280 MIPI-DSI panel.
- **Integration** — `photoframe_bridge`, a HACS-installable Home Assistant
  custom integration.

## How it works

```
  Google Photos / media sources / S3 ...
                 │
                 ▼
        Home Assistant  ── holds every credential
        photoframe_bridge
                 │  prepares each photo: orient, crop,
                 │  resize to 1280x800, baseline JPEG
                 ▼
        local network only
                 │  control: WebSocket (frame dials out)
                 │  photos:  HTTP GET from Home Assistant
                 ▼
            The frame  ── caches to SD, decodes in hardware
```

Two properties fall out of that split and drive most of the design:

**The frame holds no third-party credentials.** It stores exactly two secrets:
the Wi-Fi password and a Home Assistant-issued frame token. A frame that is
stolen, reset, or re-gifted carries no account with it. This is enforced by a
test, not just intention.

**The frame keeps working when Home Assistant does not.** Prepared photos live
on the SD card, so a Home Assistant restart, a network outage, or a power cut
does not interrupt the slideshow.

**The frame works with no Home Assistant at all.** The SD card has a `media`
folder with a plain-language note in it. Copy photos straight into that folder
and the frame shows those and nothing else -- no Wi-Fi, no adoption, no Home
Assistant. Empty the folder and restart to hand control back.

Photos put there have not been through Home Assistant's preparation step, so
the frame decodes and fits them itself: close to the panel's shape, it crops;
anything else -- a phone's portrait photos, mostly -- is scaled to fit and
centred on black rather than cropped through faces. JPEG and PNG only; HEIC,
which is what an iPhone produces by default, needs exporting as JPEG first.

## Status

Under active development against
[`specs/001-ha-managed-photo-frame`](specs/001-ha-managed-photo-frame/). Setup
and adoption are being rebuilt; see
[`tasks.md`](specs/001-ha-managed-photo-frame/tasks.md) for what is done and
what is next.

The previous on-device Google sign-in has been removed — Home Assistant now
owns all photo sourcing and authentication.

## Hardware

Shenzhen Jingcai **JC8012P4A1C_I_W_Y**: ESP32-P4 (dual-core RISC-V, 400 MHz),
32 MB PSRAM, 16 MB flash, ESP32-C6 radio co-processor over SDIO, JD9365 panel on
2-lane MIPI-DSI, GSL3680 touch, SDMMC TF card slot.

[`docs/Hardware-Reference.md`](docs/Hardware-Reference.md) is the authoritative
pinout, derived from the vendor schematics. Read it before touching a GPIO — the
package pin numbers and GPIO numbers do not match, and that has already caused
two documented errors.

## Development

### Prerequisites

- The pinned Rust ESP toolchain (`rust-toolchain.toml`)
- [`uv`](https://docs.astral.sh/uv/)
- Python 3.12 at `/usr/bin/python3` for the ESP-IDF build environment

### Host crates

`frame-core`, `frame-api`, `frame-net`, `frame-ha-bridge`, and
`frame-captive-portal` build and test on the host:

```bash
make check-host
make test-host
make lint
```

`frame-ui` and `frame-firmware` require the ESP-IDF target. On the host, slint
pulls a system `fontconfig` dependency we deliberately do not take, so they are
excluded from host checks by design.

`make test-host` needs a CPython that ships `libpython` for `pyo3`; it finds one
through `uv` automatically (`uv python install 3.13`), avoiding a system
`libpython3-dev` package.

### Firmware

```bash
bash ./scripts/bootstrap-env.sh   # one-time ESP-IDF environment
make build
make flash                        # FLASH_PORT=/dev/ttyUSB0 by default
make monitor
make dev                          # flash + monitor
```

To enter download mode, hold **BOOT** (SW3) while tapping **RESET** (SW2).

### The Home Assistant integration

Install through HACS as a custom repository, or symlink it for development:

```bash
ln -s "$PWD/custom_components/photoframe_bridge" \
      "$HA_CONFIG/custom_components/photoframe_bridge"
```

```bash
uv run --group dev pytest tests/ -q
```

## Workspace layout

| Path | What it is |
|---|---|
| `custom_components/photoframe_bridge/` | The Home Assistant integration (at the repo root, where HACS requires it) |
| `packages/frame-core/` | Shared state and the control-protocol types — the protocol's source of truth |
| `packages/frame-api/` | HTTP client for fetching prepared photos from Home Assistant |
| `packages/frame-net/` | Wi-Fi bring-up, provisioning, and discovery |
| `packages/frame-ui/` | Slint setup screens and the display/input adapters |
| `packages/frame-firmware/` | Firmware composition root |
| `packages/frame-captive-portal/` | Wi-Fi-only setup page (fallback provisioning path) |
| `packages/frame-ha-bridge/` | PyO3 bindings exposing the protocol types to Python |
| `docs/` | Hardware reference, bring-up checklist, board specification |
| `specs/` | Spec-kit specifications, plans, and tasks |

## Contributing

This project uses [spec-kit](https://github.com/github/spec-kit). Work is
specified before it is built, and
[`.specify/memory/constitution.md`](.specify/memory/constitution.md) records the
principles every spec and plan is checked against.

`/source` holds the vendor's board documentation. It is deliberately
git-ignored — it is reference material, not ours to redistribute.
