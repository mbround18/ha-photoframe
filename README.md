# Photo Frame

Embedded photo frame workspace targeting an ESP32-P4 with a Slint-based UI,
platform-neutral core state, and explicit host and firmware validation paths.

## Quick Start

### Prerequisites

- Rust ESP toolchain configured for `riscv32imafc-esp-espidf`
- `uv`
- Python 3.12 available at `/usr/bin/python3`

### Bootstrap The ESP-IDF Python Environment

The firmware build expects a Python environment at
`~/.espressif/python_env/idf5.5_py3.12_env`. Create it with:

```bash
bash ./scripts/bootstrap-env.sh
```

That script uses `uv` to create the environment with `pip`, which avoids the
system `python3.12-venv` dependency that `esp-idf-sys` would otherwise try to
use. It also installs the `ldproxy` linker wrapper into `~/.cargo/bin`, which
the ESP-IDF Rust target uses during final linking.

### Validate The Host Workflow

```bash
cargo check
cargo check --workspace --target x86_64-unknown-linux-gnu
```

### Validate The Firmware Workflow

```bash
cargo firmware-check
cargo firmware-build
```

Firmware OAuth credentials are baked into the binary from the workspace `.env`
file during build. Set `GOOGLE_OAUTH_CLIENT_ID` and
`GOOGLE_OAUTH_CLIENT_SECRET` in `.env` before running `make build`,
`cargo firmware-build`, or `make dev`.

If `.env` also contains `WIFI_SSID` and optional `WIFI_PASSWORD`, the firmware
will try that network first on boot and skip the captive Wi-Fi setup flow when
the connection succeeds.

## Workspace Layout

- `frame-core`: shared state and models that remain host-testable
- `frame-api`: protocol-oriented API client surface
- `frame-net`: networking and provisioning state surfaces
- `frame-ui`: Slint UI and hardware-facing display/input adapters
- `frame-firmware`: composition root for the ESP32-P4 firmware

## Current Integration Slice

The first implementation slice proves that firmware can compose shared state and
UI without pulling in networking, OAuth, or display driver complexity yet.

- `frame-core` owns the app phase state
- `frame-ui` renders the current phase through Slint
- `frame-firmware` creates state, updates the UI, logs the phase transition, and
  enters the Slint event loop
