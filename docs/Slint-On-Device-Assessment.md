# Slint On-Device Assessment

This note evaluates the current Slint-on-device path against the confirmed JC8012P4A1C_I_W hardware facts and identifies the most likely remaining blocker.

## What The Spec Clarifies

- The panel is natively 800 x 1280 portrait.
- The display controller is JD9365 on a MIPI DSI path.
- The board has 32 MB PSRAM, so software rendering plus staging buffers is practical.
- Touch is capacitive over I2C with dedicated reset and interrupt lines.

These points validate the current development direction. The hardware is capable of running a software-rendered Slint UI, but it does not remove the need for a board-specific embedded platform and presentation path.

## What The Current Code Already Does

- `packages/frame-ui/src/adapter.rs` creates a real embedded Slint `MainWindow` on ESP-IDF.
- `packages/frame-ui/src/display.rs` initializes an embedded display and presents RGB565 frames.
- `packages/frame-firmware/components/frame_embedded_ui/frame_embedded_ui.c` bridges Rust-rendered frames to the BSP panel handles.
- The raw panel bridge now falls back to software rotation because the current panel path rejects `esp_lcd_panel_swap_xy()`.
- The embedded Slint renderer now uses line-by-line rendering plus periodic watchdog servicing to reduce startup reset risk.

## What Is Still Missing

### 1. A stable presentation contract

The spec confirms the panel is portrait-native, while the current UI is landscape-first at 1280 x 800. That is workable, but the project still needs one durable rule for where rotation happens.

Current state:

- Slint renders a 1280 x 800 logical frame.
- The panel bridge rotates it into 800 x 1280 when hardware axis swap is unavailable.

Risk:

- Any mismatch in orientation assumptions between render, touch, and panel code will keep producing fragile behavior.

### 2. Touch integration is not real yet

`packages/frame-ui/src/input.rs` is still a placeholder. The spec helps here because the board wiring is now explicit, but the device is not yet truly Slint-native until pointer events are delivered in the correct logical orientation.

### 3. Panel transfer cost is still the main runtime risk

The most likely remaining blocker is no longer generic Slint startup. It is the total cost of rendering and moving a rotated full-screen RGB565 frame through the current raw panel bridge during early boot.

Why this remains the top suspect:

- The panel init now succeeds.
- The prior `swap_xy` failure is already handled.
- The device was still resetting after successful panel initialization.
- The render path is still doing expensive work: Slint software rendering, possible full-frame software rotation, then a full-frame panel transfer.

That means the next failure, if it persists, is most likely in one of these buckets:

- The first full-screen present is still too expensive for the startup task budget.
- The panel draw call blocks longer than expected.
- The raw panel path expects a different buffering or locking model than the current bridge provides.

## Most Likely Runtime Blocker

The most likely blocker is the full-frame presentation model, not Slint itself.

In concrete terms:

- Slint on ESP32-P4 is viable here.
- The board spec supports that conclusion.
- The fragile part is the current end-to-end path of render -> rotate -> draw bitmap during boot.

If the device still resets after the recent watchdog changes, the next thing to treat as guilty is the presentation strategy, especially the cost of rotating and blitting a full 1280 x 800 frame every time the screen changes.

## Clearest Development Path From Here

### Keep

- Slint as the long-term on-device UI.
- Rust as the source of truth for UI state.
- The BSP bridge only for board I/O.

### Change

- Make portrait-native panel assumptions explicit in the device renderer contract.
- Finish touch mapping against the same orientation contract.
- Reduce dependence on whole-frame rotated presentation when possible.

### Next technical checkpoints

1. Verify the device gets past first frame with the current watchdog-friendly renderer.
2. If it still resets, instrument whether the reset happens before or after `frame_embedded_panel_present()` returns.
3. If it happens during present, move toward a more streaming-oriented presentation path instead of full-frame rotation and blit.
4. Bring touch online only after display orientation is settled.

## Bottom Line

The new manufacturer data clears the strategic path.

- It confirms we are targeting the right hardware assumptions.
- It confirms memory is sufficient for the current software-rendered approach.
- It confirms touch integration is feasible with known board pins.

What it does not change is the tactical blocker: the embedded Slint path is now mostly a display-bridge and presentation problem, not a question of whether Slint can run on this board.
