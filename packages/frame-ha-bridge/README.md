# frame-ha-bridge

frame-ha-bridge is the Home Assistant companion package for the PhotoFrame thin-client protocol.

It provides:

- a native Rust parser for inbound control payloads
- typed Python enums and dataclasses for Home Assistant code
- helper builders for render and device-command payloads
- status-payload parsing and a lightweight frame session helper for HA-side orchestration
- a packaged Home Assistant custom-component payload under `homeassistant/custom_components/photoframe_bridge`

## Install

Build and install directly from the package directory:

```bash
cd packages/frame-ha-bridge
uv pip install -e .
```

For editable local development:

```bash
cd packages/frame-ha-bridge
maturin develop
```

## Home Assistant Package Artifact

From the repository root, build the uploadable Home Assistant archive with:

```bash
make package
```

That emits a `.tgz` in `./dist` containing:

```text
custom_components/photoframe_bridge/
```

The archive is intended to be extracted directly inside Home Assistant's
`custom_components` directory.

## Example

```python
from frame_ha_bridge import DeviceCommand, TransitionType
from frame_ha_bridge import build_command_payload, build_render_payload, parse_control_payload

payload = build_render_payload(
    "https://example.com/photo.jpg",
    transition_type=TransitionType.FADE,
    brightness=60,
    correlation_id="media-42",
)

message = parse_control_payload(payload)
assert message.kind == "render"
assert message.transition_type == TransitionType.FADE

command = build_command_payload(DeviceCommand.RELOAD_UI, correlation_id="cmd-7")
```

## Controller Example

```python
from frame_ha_bridge import FrameSession, ScreenStatus, parse_status_payload

session = FrameSession()

connected = session.apply_status_payload(
    '{"type":"connected","device_id":"esp32p4-abcd","device_name":"Kitchen Frame"}'
)
assert session.device_id == "esp32p4-abcd"

render_payload = session.build_render_payload(
    "https://example.com/photo.jpg",
    correlation_id="render-1",
)

health = parse_status_payload(
    '{"type":"health","health":{"screen_status":"rendering"}}'
)
assert health.screen_status == ScreenStatus.RENDERING
```

## Home Assistant usage

The intended MVP integration pattern is:

1. Home Assistant constructs JSON payloads with this package.
2. The ESP thin client receives only resolved media URLs plus metadata over the control channel.
3. The device reports typed status payloads back to Home Assistant.
4. The HA side owns Google auth, token refresh, album selection, and read-only media source access.

That keeps the Home Assistant package and the device firmware aligned on one protocol contract while moving provider-specific logic off-device.

