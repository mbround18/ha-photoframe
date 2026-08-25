# Phase 1 Data Model: Home Assistant-Managed Digital Photo Frame

**Feature**: `001-ha-managed-photo-frame` | **Date**: 2026-08-25

Entities from [spec.md](./spec.md) rendered as concrete structures, with the storage location and
lifetime of each. "HA" means Home Assistant; "frame" means on-device.

---

## Storage map

| Store | Location | Holds | Survives |
|---|---|---|---|
| Config entry data | HA `.storage` | Frame identity, frame token, provider credentials | HA restart; deleted with the entry |
| Config entry options | HA `.storage` | Selection, presentation settings | HA restart |
| Prepared-photo cache | HA config dir, `photoframe_bridge/photos/` | Prepared JPEG bytes, content-addressed | HA restart; LRU-evicted |
| Runtime state | HA memory | Pool, cursor, connection state | Nothing — rebuilt on reload |
| Frame NVS | Frame flash, `nvs` partition | Wi-Fi credential, controller binding, presentation settings | Reboot, power loss |
| Frame SD cache | `/sdcard/photoframe/` | Prepared JPEGs + index | Reboot, power loss, card reseat |

---

## Home Assistant side

### Frame (one config entry per frame)

One physical device. Created by the adoption config flow, backed by a config entry and an HA device
registry entry.

| Field | Type | Notes |
|---|---|---|
| `frame_id` | str | Stable device identity. Derived from the P4's eFuse MAC. Config entry `unique_id`. |
| `name` | str | Owner-assigned at adoption. |
| `frame_token` | str (secret) | Issued by HA at adoption; the frame presents it on the control channel and on photo fetches. Rotatable. |
| `panel_width`, `panel_height` | int | Advertised by the frame at connect. 1280x800 on this hardware. Never assumed. |
| `firmware_version` | str | Reported at connect; shown as a device attribute. |
| `last_seen` | datetime | Updated by control-channel traffic. |
| `connection_state` | enum | `online` \| `offline` \| `adopting` |

**Validation**: `frame_id` unique across entries (FR-005, FR-006). A discovery for an existing
`frame_id` updates the host rather than creating a second entry (edge case: frame changes IP).

**State transitions**:

```
discovered ──adopt──> adopting ──frame connects & claim accepted──> online
   ^                                                                  │
   │                                             control channel drops │
   └──────────── entry removed / frame reset ──── offline <───────────┘
```

---

### PhotoSource

A configured connection to somewhere photos live. Owns the credential. May serve many frames.

| Field | Type | Notes |
|---|---|---|
| `source_id` | str | Stable id. |
| `provider_key` | str | `media_source` \| `google_photos_picker` \| `sample` |
| `title` | str | Shown to the owner. |
| `credential_ref` | opaque \| None | For OAuth providers, the HA `application_credentials` / OAuth session reference. **Never leaves HA** (Principle II, FR-008, FR-043). |
| `capabilities` | Capabilities | Copied from the provider at registration; see [photo-provider.md](./contracts/photo-provider.md). |
| `health` | enum | `ok` \| `needs_reauth` \| `unavailable`, with a human-readable reason. |

**Validation**: a source failure sets `health` on that source only and must not affect other
sources or frames (FR-017).

---

### Collection

A named grouping inside a source. Not every source has them (`sample` does not; a Picker session
exposes one synthetic collection).

| Field | Type | Notes |
|---|---|---|
| `collection_id` | str | Provider-scoped. |
| `source_id` | str | Owner. |
| `title` | str | |
| `item_count` | int \| None | `None` when the provider cannot count cheaply. |
| `cover_item_id` | str \| None | For the picker UI. |

---

### Selection

What the owner chose for one frame. Belongs to exactly one frame; stored in config entry options.

| Field | Type | Notes |
|---|---|---|
| `source_id` | str | |
| `collection_ids` | list[str] | May be empty if `item_ids` is used (FR-011). |
| `item_ids` | list[str] | Explicitly picked photos. |
| `expires_at` | datetime \| None | Set when `capabilities.selection_expires`. Drives the re-pick repair flow (R2). |

**Validation**: at least one of `collection_ids` / `item_ids` must be non-empty. An empty resolved
pool must be reported at selection time, not discovered later as a blank frame (edge case).

---

### PhotoPool

The resolved photo list for one frame. Runtime only — rebuilt on reload.

| Field | Type | Notes |
|---|---|---|
| `items` | list[PhotoRef] | Deduplicated across collections (edge case: same photo in two albums). |
| `cursor` | int | Position in the play order. |
| `order` | enum | `shuffle` \| `chronological` \| `random` |
| `shuffle_seed` | int | Makes shuffle reproducible so the no-repeat guarantee (FR-015) survives a reload. |
| `refreshed_at` | datetime | |
| `next_refresh_at` | datetime \| None | `None` when `not capabilities.supports_live_collections` — a frozen selection is never re-polled (R2). |

**Invariant (FR-015)**: no photo repeats until the pool is exhausted. Implemented as a permutation
over indices, not by random draw.

### PhotoRef

| Field | Type | Notes |
|---|---|---|
| `item_id` | str | Provider-scoped, stable. |
| `source_id` | str | |
| `created_at` | datetime \| None | Drives chronological order. |
| `mime_type` | str | Non-image types are filtered out of the pool (FR-018). |
| `width`, `height` | int \| None | Used to pick the fetch size before downloading. |

---

### PreparedPhoto

One photo processed for one frame geometry. The delivered and cached unit.

| Field | Type | Notes |
|---|---|---|
| `photo_id` | str | `sha256(item_id + source_id + geometry + pipeline_version)`, truncated. Content-addressed, so preparation is idempotent and cacheable. |
| `bytes_path` | Path | File in the HA prepared-photo cache. |
| `byte_size` | int | |
| `geometry` | (int, int) | The frame geometry it was prepared for. A second frame with a different panel gets its own `photo_id`. |
| `treatment` | enum | `fill` \| `letterbox_blur` — which path FR-022 took. |
| `prepared_at` | datetime | |
| `source_ref` | PhotoRef | For attribution and re-preparation. |

**Invariants**:
- Encoded **baseline** JPEG, `progressive=False`. The frame's hardware decoder rejects progressive
  (R4). Enforced by a test that re-parses the encoded output.
- Exactly `geometry` pixels. The frame does no resizing (FR-019, Principle VI).
- EXIF orientation already applied and stripped (FR-020).
- `pipeline_version` participates in `photo_id`, so changing the pipeline invalidates the cache
  rather than silently serving stale renders.

---

### PresentationSettings

Per-frame, in config entry options. Mirrored to frame NVS so they survive a power cycle without HA
(FR-033).

| Field | Type | Default | Entity |
|---|---|---|---|
| `rotation_interval_s` | int | 300 | `number` |
| `brightness` | int 0-100 | 80 | `number` |
| `order` | enum | `shuffle` | `select` |
| `transition` | enum | `fade` | `select` |
| `screen_on` | bool | `true` | `switch` |
| `paused` | bool | `false` | `switch` |
| `pool_refresh_interval_s` | int | 3600 | `number`; ignored when the selection is frozen |
| `cache_target_count` | int | 500 | `number` |

---

## Frame side

### FrameIdentity (NVS)

| Field | Notes |
|---|---|
| `frame_id` | eFuse-MAC-derived. Read-only, survives reset. |
| `wifi_ssid`, `wifi_psk` | Secret. Cleared by reset (FR-040, FR-042). |
| `controller_host`, `controller_port` | Learned at adoption; re-resolved via mDNS if the host moves. |
| `frame_token` | Secret. Cleared by reset. |
| `adopted` | bool. False means run the first-run experience. |

**Invariant (FR-006)**: while `adopted` is true, a claim from any other controller is refused.

### CacheIndex (SD, `/sdcard/photoframe/index.json`)

| Field | Notes |
|---|---|
| `entries` | `photo_id` → `{ path, byte_size, last_shown_at, added_at, verified }` |
| `target_count` | LRU cap; default 500 (R5). |

**Invariants**:
- Photos are written to a temp name and renamed atomically, so a power loss mid-write cannot leave a
  half photo to display (edge case).
- `verified` flips true only after the file's size and JPEG SOI/EOI markers check out on load.
- Eviction is LRU by `last_shown_at`, never dropping the currently-displayed or next-prefetched
  photo.
- A missing, full, or unreadable card degrades to an in-memory ring of a few photos rather than
  stopping (FR-030).

### SlideshowState (RAM)

| Field | Notes |
|---|---|
| `current_photo_id`, `next_photo_id` | `next` is decoded and staged before it is needed (FR-024). |
| `playing` | |
| `last_advance_at` | |
| `source` | `controller` \| `cache_only` — set to `cache_only` when the control channel is down, and the slideshow continues regardless (FR-026). |

---

## Cross-cutting rules

1. **No third-party credential ever appears in a frame-side structure.** The frame's only secrets
   are `wifi_psk` and `frame_token` (Principle II, FR-008).
2. **`photo_id` is the only identifier crossing the wire for a photo.** Provider item ids, source
   URLs, and account identifiers stay in Home Assistant (FR-043, Principle IV).
3. **Everything the frame needs to keep running is on the frame.** Presentation settings in NVS,
   photos on SD — so a power-cycled frame with no controller still shows photos (FR-027, SC-007).
4. **Geometry is advertised, never assumed.** The frame reports its panel size at connect and Home
   Assistant prepares to that, so a different panel needs no Home Assistant change.
