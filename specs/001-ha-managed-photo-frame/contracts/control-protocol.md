# Contract: Frame ↔ Home Assistant Control Protocol

**Feature**: `001-ha-managed-photo-frame`

Two channels, both local-network only (Principle IV):

- **Control** — a WebSocket the frame opens to Home Assistant. Small JSON messages.
- **Photo transfer** — plain HTTP GET from the frame to Home Assistant. Prepared JPEG bytes.

The frame is always the client on both. Home Assistant never dials the frame, so the frame needs no
open port and no stable IP.

**Source of truth**: the Rust types in `frame-core/src/control.rs`, exposed to Python by
`frame-ha-bridge` and mirrored dependency-free in `custom_components/photoframe_bridge/protocol.py`.
A CI test asserts the mirror matches the Rust definitions.

This extends the protocol already in the tree rather than replacing it; `media_url`, `cmd`,
`transition_type`, `brightness`, `correlation_id`, and `ControllerRegistration` keep their current
meanings.

---

## Control channel

- **Endpoint**: `ws://<ha-host>:8765/ws` (port configurable per config entry).
- **Auth**: the frame sends `hello` with its `frame_token` as its first frame. A connection that has
  not authenticated within 5 seconds is closed.
- **Framing**: one JSON object per WebSocket text message. No batching.
- **Keepalive**: WebSocket ping every 30 s; three missed pongs closes the connection.
- **Reconnect**: exponential backoff from 1 s to 60 s with jitter, forever. The slideshow runs
  from cache throughout (FR-026).

### Frame → Home Assistant

#### `hello`

First message on every connection.

```json
{
  "type": "hello",
  "frame_id": "p4-a1b2c3d4e5f6",
  "frame_token": "<secret>",
  "device_name": "Living Room Frame",
  "firmware_version": "0.2.0",
  "panel": { "width": 1280, "height": 800 },
  "cache": { "count": 412, "capacity": 500 },
  "protocol_version": 2
}
```

`panel` is authoritative — Home Assistant prepares photos to whatever the frame reports and never
assumes a geometry (data-model cross-cutting rule 4).

#### `status`

Existing message. Types: `render_started`, `render_completed`, `command_acknowledged`, `health`,
`error`. Extended with:

```json
{ "type": "cache_report", "count": 412, "capacity": 500,
  "have": ["<photo_id>", "..."], "missing": ["<photo_id>"] }
```

Sent after `hello` and whenever the cache changes materially. It lets Home Assistant avoid pushing
photos the frame already holds.

```json
{ "type": "showing", "photo_id": "ab12cd34", "since": "2026-08-25T18:03:11Z" }
```

Drives the "what is it showing" entity (FR-031).

### Home Assistant → Frame

#### `claim`

Response to `hello`. Existing `ControllerRegistration`, extended.

```json
{ "type": "claim", "claimed": true, "display_name": "Living Room Frame",
  "settings": { "rotation_interval_s": 300, "brightness": 80,
                "order": "shuffle", "transition": "fade",
                "screen_on": true, "paused": false } }
```

`claimed: false` with a `message` when the frame is bound elsewhere (FR-006). Settings are persisted
to NVS so they survive a power cycle without Home Assistant (FR-033).

#### `enqueue`

Replaces the old "render this URL" as the normal path. Home Assistant tells the frame what to have
ready; the frame fetches on its own schedule.

```json
{
  "type": "enqueue",
  "photos": [
    { "photo_id": "ab12cd34",
      "url": "/api/photoframe_bridge/photo/ab12cd34",
      "byte_size": 184320,
      "sha256": "…" }
  ],
  "evict": ["ff99aa00"],
  "correlation_id": "pool-42"
}
```

`url` is always a **path on Home Assistant**, never a provider URL — the frame never learns where a
photo came from (FR-043, Principle II/IV). `sha256` lets the frame reject a truncated download
rather than displaying a corrupt photo.

#### `show`

Display one specific photo now, then resume rotation (FR-036).

```json
{ "type": "show", "photo_id": "ab12cd34",
  "transition": "fade", "correlation_id": "svc-7" }
```

If the frame does not hold it, it fetches it first; the transition waits rather than showing a gap.

#### `cmd`

Existing device commands, extended: `reboot`, `reload_ui`, `next`, `previous`, `pause`, `resume`,
`screen_on`, `screen_off`, `factory_reset`.

#### `settings`

A settings delta, applied live and persisted to NVS (FR-033).

---

## Photo transfer

```
GET /api/photoframe_bridge/photo/{photo_id}
Authorization: Bearer <frame_token>
```

| Response | Meaning | Frame behaviour |
|---|---|---|
| `200` + `image/jpeg` | Prepared photo | Verify `sha256`, write to a temp file, `rename` atomically, index it |
| `401` | Bad or rotated token | Drop the control connection and re-`hello` |
| `404` | Evicted from the HA cache | Drop the `photo_id` from the queue; do not retry |
| `503` | Preparation still in progress | Retry with backoff; keep showing what it has |

**Guarantees on the bytes** (from data-model `PreparedPhoto`, enforced by tests):

- Baseline JPEG, **never progressive** — the P4's hardware decoder rejects progressive (research R4).
- Exactly the frame's advertised geometry. The frame does no scaling (Principle VI).
- EXIF orientation already applied and metadata stripped.
- `Content-Length` always set, so the frame can size its buffer before reading.

Range requests are not supported; prepared photos are ~200 KB and a failed download is simply
retried.

---

## Ordering and failure rules

1. **The frame is authoritative about what it is showing.** Home Assistant reflects `showing`; it
   never assumes a `show` took effect.
2. **`correlation_id` echoes on every response**, so a service call can be awaited (FR-035).
3. **Unknown message types are ignored, not errors** — this is how the protocol stays
   forward-compatible across mismatched firmware and integration versions.
4. **`protocol_version` mismatch**: Home Assistant serves the highest version the frame declares
   support for. A frame too old to understand `enqueue` falls back to the existing `media_url`
   render path.
5. **No message ever carries photo bytes.** Bytes move over HTTP only, so a slow transfer cannot
   block control (research R4 alternatives).
6. **A control-channel failure is never visible on the panel** (Principle VIII, FR-037). The frame
   switches to `cache_only` and keeps going.
