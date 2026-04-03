# Board Bring-Up Checklist

This checklist turns the confirmed manufacturer hardware facts for the JC8012P4A1C_I_W into a concrete bring-up path for this repository.

## Confirmed Hardware Facts

- Main MCU: ESP32-P4
- Wireless coprocessor: ESP32-C6 over SDIO/UART
- Display panel: JD9365 over MIPI DSI
- Native panel resolution: 800 x 1280 portrait
- External memory: 32 MB PSRAM
- Touch: capacitive touch over I2C
- Board-level display/touch pins:
  - LCD reset: GPIO27
  - LCD backlight PWM: GPIO25
  - Touch interrupt: GPIO21
  - Touch reset: GPIO24

## Bring-Up Order

The intended order is display power and panel control first, then visible test output, then touch, then Slint interaction, then optimization.

## 1. Display and Backlight

- Verify BSP initialization succeeds consistently through the raw display path.
- Verify panel reset sequencing matches the board support package for GPIO27.
- Verify backlight comes on through the BSP or PWM path on GPIO25.
- Verify the panel can accept a known-good raw frame before adding more UI complexity.

Acceptance criteria:

- Board boots without resetting.
- Backlight is visibly on.
- A static full-screen test image can be shown repeatedly.

## 2. Native Orientation

- Treat the panel as native portrait 800 x 1280.
- Keep the logical UI orientation decision explicit.
- If the UI remains landscape 1280 x 800, keep rotation in one place only.
- Avoid mixing panel mirroring, axis swapping, and application-side assumptions across multiple layers.

Acceptance criteria:

- One documented orientation contract exists.
- A test pattern renders with the expected top, bottom, left, and right edges.
- No duplicated rotation logic exists in both the panel bridge and the UI layer.

## 3. Slint Render Path

- Confirm the embedded Slint platform initializes successfully.
- Confirm the first frame reaches the panel without watchdog reset.
- Confirm the renderer can repaint more than one frame in sequence.
- Confirm the software framebuffer and any rotation buffer are allocated from PSRAM-capable memory when possible.

Acceptance criteria:

- Device survives first paint.
- Device survives repeated repaint requests.
- Logs show successful frame presentation beyond the first frame.

## 4. Touch Controller Bring-Up

- Identify the exact touch controller used on the capacitive panel path.
- Initialize the I2C bus used by the touch controller.
- Wire touch reset on GPIO24.
- Wire touch interrupt on GPIO21.
- Convert raw touch coordinates into the same logical orientation used by the UI.
- Feed pointer events into the embedded Slint window.

Acceptance criteria:

- Touch reset and interrupt lines behave as expected.
- At least one touch point is detected reliably.
- Touch coordinates align with the displayed UI orientation.
- A simple Slint tap target reacts on-device.

## 5. UI State Validation

- Verify splash, setup, authorization, browser pairing, and ready states render correctly on hardware.
- Verify state changes repaint without tearing or lockups.
- Verify text-heavy screens remain legible after any scaling or rotation logic.

Acceptance criteria:

- Each app phase renders on the physical panel.
- State transitions do not reset or freeze the device.
- Pairing and setup copy remain readable on the native panel.

## 6. Performance and Stability

- Measure first-frame latency.
- Measure steady-state repaint cost.
- Confirm watchdog servicing is no longer needed as a crutch for obviously pathological work, or document why it remains necessary.
- Prefer line-by-line or DMA-friendly presentation if full-frame blits stay expensive.

Acceptance criteria:

- No watchdog resets during startup or normal repaint.
- Frame updates are stable enough for the setup flow.
- CPU and memory usage are acceptable for running Wi-Fi, captive portal, and UI together.

## 7. Integration Cleanup

- Once Slint is stable on device, remove duplicated embedded LVGL-specific UI responsibilities.
- Keep only the minimum BSP bridge needed for panel and touch I/O.
- Preserve a single source of truth for UI state in Rust.

Acceptance criteria:

- Device UI flow is driven from the Slint path.
- Legacy LVGL UI code is no longer the primary renderer.
- Orientation, touch mapping, and presentation behavior are documented.

## Code Areas To Use During Bring-Up

- `packages/frame-ui/src/adapter.rs`
- `packages/frame-ui/src/display.rs`
- `packages/frame-ui/src/input.rs`
- `packages/frame-firmware/components/frame_embedded_ui/frame_embedded_ui.c`
- `packages/frame-firmware/src/main.rs`
