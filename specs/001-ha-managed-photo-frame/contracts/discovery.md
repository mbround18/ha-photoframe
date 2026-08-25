# Contract: Provisioning, Discovery, and Adoption

**Feature**: `001-ha-managed-photo-frame` | **Spec**: User Story 1 | **Constitution**: Principle V

Three stages get a factory-fresh frame to "showing photos", with no companion app at any point:

```
  powered on, no Wi-Fi          on Wi-Fi, unadopted            adopted
 ┌──────────────────┐   Improv   ┌─────────────────┐  zeroconf  ┌──────────┐
 │  BLE advertising │ ─────────> │ mDNS announcing │ ─────────> │ claimed  │
 │  (stage 1)       │  Wi-Fi     │  (stage 2)      │  + claim   │ (stage 3)│
 └──────────────────┘            └─────────────────┘            └──────────┘
```

---

## Stage 1: Wi-Fi provisioning

### Primary: Improv Wi-Fi over BLE

The frame implements the [Improv BLE](https://www.improv-wifi.com/ble/) GATT service. Home
Assistant's `improv_ble` integration discovers and provisions it; `improv-wifi.com` works as a
browser-based fallback for owners whose HA host has no Bluetooth.

- **Service UUID**: `00467768-6228-2272-4663-277478268000`
- **Characteristics**: Capabilities, Current State, Error State, RPC Command, RPC Result
- **States**: `authorized` → `provisioning` → `provisioned` (or `error`)
- **Advertising**: only while `adopted == false`. An adopted frame stops advertising entirely — no
  standing BLE attack surface in a living room.
- **Errors**: a wrong password returns `unable_to_connect` and the frame returns to `authorized`,
  ready for an immediate retry with no reset and no power cycle (FR-002).

**Precondition**: this depends on BLE HCI reaching the ESP32-C6 over SDIO via `esp-hosted-mcu`.
Espressif documents the capability, but it is **unproven on this board's factory C6 firmware**. The
M2 spike decides. See [research.md](../research.md) R3.

### Fallback: Wi-Fi-only SoftAP page

Built **only if the M2 spike fails**.

- SSID `PhotoFrame-XXXX` (last 4 of `frame_id`), open, no password.
- Captive-portal redirect to a single page whose only function is joining a network.
- The AP is torn down permanently the moment Wi-Fi is joined and never returns unless the frame is
  reset.
- Reuses `frame-captive-portal` with the Google OAuth routes deleted.

Still satisfies FR-001 — no companion app, no cloud, no typed address.

### On-screen during stage 1

The one moment a screen is permitted to show setup content (Principle VIII governs the *adopted*
frame). Even here: plain language, no IP addresses, no error codes, no logs. "Open Home Assistant to
set up this frame" — not "AP started on 192.168.71.1".

---

## Stage 2: mDNS announcement

Once on Wi-Fi the frame announces continuously, adopted or not, so Home Assistant can rediscover it
after a router reassigns its address (FR-005, edge case "HA unreachable at the adopted address").

- **Service type**: `_photoframe._tcp.local.`
- **Instance name**: `PhotoFrame <frame_id-suffix>` — stable across reboots. Not session-scoped.
- **Port**: the frame's HTTP port (used only for identity probes; control is frame-initiated).

**TXT records**:

| Key | Example | Purpose |
|---|---|---|
| `frame_id` | `p4-a1b2c3d4e5f6` | Config entry `unique_id` |
| `fw` | `0.2.0` | Firmware version |
| `panel` | `1280x800` | Lets the flow show geometry before adoption |
| `adopted` | `0` / `1` | Home Assistant hides an already-adopted frame from discovery |
| `proto` | `2` | Protocol version |

`manifest.json` declares `"zeroconf": ["_photoframe._tcp.local."]`, so Home Assistant surfaces a
discovery card with no user action (FR-003).

---

## Stage 3: Adoption

### Config flow

1. `async_step_zeroconf` — parse TXT, set `unique_id = frame_id`,
   `_abort_if_unique_id_configured(updates={CONF_HOST: host})` so a moved frame updates in place
   instead of duplicating.
2. If `adopted == 1` and it is not ours, abort with `already_adopted` (FR-006).
3. `async_step_confirm` — show the frame's name and panel size; the owner names it.
4. Home Assistant mints a `frame_token`, creates the config entry, and waits for the frame to
   connect and be claimed.
5. Optionally chain straight into photo-source setup; if skipped, the frame shows the bundled
   `sample` photos so it is never blank (research R1).

Manual entry by host is also offered, for VLANs where mDNS does not cross subnets.

### The claim handshake

Binding happens on the control channel, not over mDNS, so the token never touches a broadcast
protocol:

```
frame                                   home assistant
  │  ws connect + hello{frame_id, frame_token=""}
  ├─────────────────────────────────────────────>
  │                     claim{claimed:true, token:"…", display_name:"…"}
  │<─────────────────────────────────────────────┤
  │  persist token + binding to NVS, adopted=1
  │  stop BLE advertising, set mDNS adopted=1
```

A frame with `adopted == true` refuses a claim carrying a different controller identity (FR-006) and
reports it so the second Home Assistant can say "already adopted; reset it first".

---

## Un-adoption and reset

| Trigger | Effect | Requirement |
|---|---|---|
| Config entry removed in HA | HA sends `factory_reset`; frame clears token + binding + SD cache, keeps Wi-Fi, resumes mDNS with `adopted=0` and BLE advertising | FR-039 |
| On-device reset | Clears Wi-Fi, token, binding, **and** the SD cache; returns to stage 1 | FR-040, FR-042 |

**On-device reset gesture** (FR-041): a hidden press-and-hold in a screen corner for 10 seconds,
followed by an explicit on-screen confirmation. Deliberately hard to hit by accident — a frame on a
wall gets dusted, bumped, and touched by children. Because it is the only touch interaction an
adopted frame has, it must not be discoverable by casual contact.

**Verification (FR-042)**: after a reset, dump NVS and the SD card and assert no token, no PSK, no
controller reference, and no photo bytes remain.

---

## Security properties

1. **The frame holds no third-party credential at any stage.** Improv carries only a Wi-Fi PSK; the
   claim carries only a Home Assistant-issued frame token (Principle II).
2. **BLE is off once adopted.** No permanent radio attack surface.
3. **The token is minted by Home Assistant, never chosen by the frame**, and is rotatable by
   reloading the config entry.
4. **mDNS TXT carries no secret** — identity and capability only.
5. **First-writer-wins adoption.** A frame cannot be silently stolen by a second Home Assistant on
   the same network.
