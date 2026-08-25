# Hardware Reference: JC8012P4A1C_I_W_Y

**Board**: Shenzhen Jingcai JC8012P4A1C_I_W_Y — 10.1" ESP32-P4 HMI module
**Derived**: 2026-08-25, from the vendor bundle under `/source` (git-ignored)

## Sources and how much to trust them

| Source | Location | Authority |
|---|---|---|
| Schematics (7 sheets) | `/source/.../5-Schematic/*.png` | **Highest** — the physical board |
| Vendor BSP + IDF 5.5.4 demos | `/source/.../1-Demo/IDF_5.5.4/` | High — code that runs on this board |
| C6 slave firmware binaries | `/source/.../8-Burn operation/Burn files/` | High — what actually ships on the C6 |
| `docs/SPECIFICATION.md` | this repo | **Contains errors** — see [Corrections](#corrections) |
| Upstream `espressif/esp32_p4_function_ev_board` BSP | component registry | Mostly right, **wrong touch driver** — see [Divergences](#divergences-from-the-espressif-ev-board) |

Everything below is cross-checked against the schematic. Where the BSP and the schematic
disagreed, the schematic won and the discrepancy is called out.

---

## 1. Complete P4 pin map

Package is QFN104. **Package pin number ≠ GPIO number** — confusing the two is the source of every
error in `docs/SPECIFICATION.md`.

### Display

| Net | GPIO | Pkg pin | Notes |
|---|---|---|---|
| `LCD_RST` | **GPIO27** | 56 | To DSI FPC1 pin 27 |
| `LCD_PWM` | **GPIO23** | 25 | Backlight. Drives MP3202 boost `EN` via R15 (0R) |
| DSI DATA0 ±, DATA1 ±, CLK ± | dedicated DSI pins | 34-40 | 2 lanes, not GPIO-muxed |

Backlight is **on/off + PWM through the boost converter's enable pin**, not a simple LED drive.
`bsp_display_backlight_on()` / `bsp_display_brightness_set()` handle it.

### Touch

| Net | GPIO | Pkg pin |
|---|---|---|
| `TOUCH_INT` | **GPIO21** | 23 |
| `TOUCH_RST` | **GPIO22** | 24 |
| I2C SDA / SCL | GPIO7 / GPIO8 | 7 / 8 |

Controller is a **GSL3680** on FPC2, address per `ESP_LCD_TOUCH_IO_I2C_GSL3680_CONFIG()`.

### The shared I2C bus (this trips people up)

The codec sheet contains an explicit net-alias table proving these are all **one physical bus**:

```
ES_I2C_SDA  =  RTC_DAT/SDA1  =  GPIO7
ES_I2C_SCL  =  RTC_CLK/SCL1  =  GPIO8
```

So `GPIO7` (SDA) and `GPIO8` (SCL) carry, simultaneously:

- ES8311 audio codec (`CCLK`/`CDATA`)
- GSL3680 touch controller (via FPC2, 5K1 pullups R53/R54)
- RX8025T-UC real-time clock
- OV02C10 camera control (via FPC5, 2K2 pullups R43/R44)
- Expansion headers CN4, FPC3

`docs/SPECIFICATION.md` lists touch on a *separate* `RTC_CLK/SCL1` bus. Those are alias names for
the same two pins, not a second bus. Combined pullups are strong (~1.3K effective) — fine at
400 kHz, but do not add more.

### Audio (ES8311 codec + NS4150 amplifier)

| Function | GPIO | Pkg pin | Direction |
|---|---|---|---|
| `CODEC_I2S0_DSDIN` | **GPIO9** | 10 | P4 → codec (`I2S DOUT`) |
| `LRCK` | **GPIO10** | 11 | Word clock |
| `ES7210_SDOUT` | **GPIO11** | 12 | codec → P4 (`I2S DIN`) |
| `SCLK` | **GPIO12** | 13 | Bit clock |
| `MCLK` | **GPIO13** | 14 | Master clock |
| `PA_CTRL` | **GPIO20** | 22 | NS4150 enable, 10K pulldown (R86) — **idles off** |

Naming is codec-side. From the P4's view GPIO9 is DOUT and GPIO11 is DIN.

### SD / TF card — SDMMC slot, 4-bit

| Net | GPIO | Pkg pin |
|---|---|---|
| `SD_DATA0` | **GPIO39** | 80 |
| `SD_DATA1` | **GPIO40** | 81 |
| `SD_DATA2` | **GPIO41** | 82 |
| `SD_DATA3` | **GPIO42** | 83 |
| `SD_CLK` | **GPIO43** | 84 |
| `SD_CMD` | **GPIO44** | 86 |

All lines carry 5K1 pullups (R47-R52). Card **VDD is `ESP_LDO_VO4`** — an internal P4 LDO channel,
so the SD rail must be brought up before mounting. There is a card-detect line on the socket.

**No conflict with the Wi-Fi co-processor.** The C6 is on a completely separate SDIO bus.

### ESP32-C6 co-processor link (ESP-Hosted over SDIO)

| Net | P4 GPIO | P4 pkg pin | C6 GPIO | C6 pkg pin |
|---|---|---|---|---|
| `SD2_D0` | **GPIO14** | 15 | IO20 | 26 |
| `SD2_D1` | **GPIO15** | 16 | IO21 | 27 |
| `SD2_D2` | **GPIO16** | 17 | IO22 | 28 |
| `SD2_D3` | **GPIO17** | 18 | IO23 | 29 |
| `SD2_CLK` | **GPIO18** | 19 | IO19 | 25 |
| `SD2_CMD` | **GPIO19** | 20 | IO18 | 24 |
| `C6_CHIP_PU` | **GPIO54** | 98 | `EN` | 8 |
| `C6_IO2` | **GPIO6** | 6 | IO2 | 5 |

**GPIO54 drives the C6's EN pin**, so the P4 *can* hard-reset the co-processor. This matters: it
means `CONFIG_ESP_HOSTED_GPIO_SLAVE_RESET_SLAVE=54` — inherited from the Espressif EV board — is
correct here by luck, and a wedged C6 can be recovered without a power cycle.

`GPIO6 ↔ C6 IO2` is a spare side-channel; ESP-Hosted over SDIO does not use it.

### Buttons, LED, battery, UART

| Function | GPIO | Detail |
|---|---|---|
| **BOOT button (SW3)** | **GPIO35** (pin 66) | Momentary to GND, 10K pullup (R40). **Active low.** |
| **RESET button (SW2)** | — | Shorts `CHIP_PU` to GND. Hard reset, not readable. |
| **Power button (SW1)** | — | IP5306 `KEY` pin. Power/charge control. |
| **WS2812 RGB LED** | **GPIO26** (pin 55) | One addressable LED, `WS2812_DAT`, from VCC3V3 |
| **Battery sense** | **GPIO52** (pin 95) | ADC via divider R2 68K / R6 100K from `BAT+` |
| `UART0_TXD` | **GPIO37** (pin 69) | CH340C console |
| `UART0_RXD` | **GPIO38** (pin 70) | CH340C console |
| `BOOTMODE` strap | GPIO35 | Same pin as the boot button |
| `EN_DCDC` | pin 79 | Enables the 1.2V `ESP_VDD_HP` rail |

**The BOOT button (GPIO35) is a real, readable, debounced-by-hardware push button.** For a device
that needs a deliberate, hard-to-trigger factory reset, this is far better than a hidden touch
gesture — especially given the touch-driver risk below.

**The WS2812 LED** is a genuinely useful find: it can signal setup progress, connection loss, and
reset confirmation *without putting any text on the panel*, which is exactly what Constitution
Principle VIII asks for.

### Free / expansion GPIOs

Broken out on FPC3, FPC4, CN4: **GPIO2, 3, 4, 5, 28, 29, 30, 31, 32, 33, 34, 45, 46, 47, 48**.
Unconnected on the die: GPIO49, GPIO50, GPIO51, GPIO53.
Reserved: GPIO0/GPIO1 (32.768 kHz crystal X2), GPIO24/GPIO25 (USB), flash pins 27-33.

---

## 2. Display subsystem

| Property | Value |
|---|---|
| Panel | 10.1" IPS, JD9365DA-H3 driver (HKC QP101BS01-1) |
| Native resolution | **800 × 1280 (portrait)** |
| Landscape canvas | 1280 × 800 |
| Interface | MIPI-DSI, **2 data lanes**, 1000 Mbps/lane |
| Refresh | 60 Hz |
| Colour | RGB565 (16 bpp) or RGB888 (24 bpp) |
| DPHY power | Internal LDO **channel 3 @ 2500 mV** |
| Init config | `JD9365_PANEL_BUS_DSI_2CH_CONFIG()`, `JD9365_800_1280_PANEL_60HZ_DPI_CONFIG()` |

Bring-up order, from the vendor's `video_lcd_display` demo:

1. `esp_ldo_acquire_channel(chan 3, 2500 mV)` — DPHY power **first**
2. `esp_lcd_new_dsi_bus()`
3. `esp_lcd_new_panel_io_dbi()`
4. `esp_lcd_new_panel_jd9365()` with `reset_gpio_num = GPIO27`
5. `esp_lcd_panel_reset()` → `esp_lcd_panel_init()`
6. Backlight on (GPIO23)

The panel is physically portrait. Landscape is achieved by rotation, and **the PPA does that in
hardware** — never in software.

---

## 3. Hardware acceleration

Both blocks the photo pipeline needs are present and are used by the vendor's own demos.

### JPEG codec (`esp_driver_jpeg`)

- **Baseline JPEG only.** Progressive JPEG will not decode. This is why the Home Assistant renderer
  must emit `progressive=False`.
- Chroma subsampling: YUV444 / YUV422 / YUV420
- Output: RGB565, RGB888, GRAY
- Throughput: ~109 fps at 1280×720, so a 1280×800 decode is roughly 8-10 ms
- Buffers must come from `jpeg_alloc_decoder_mem()`; output dimensions pad to 16-byte multiples
- Encoder and decoder are mutually exclusive — one mode at a time

### PPA — Pixel Processing Accelerator (`esp_driver_ppa`)

`ppa_do_scale_rotate_mirror()` performs scale, rotate, and mirror in hardware. The vendor's
`video_lcd_display` demo drives the panel through it every frame.

**Consequence for this project**: the photo path is `hardware JPEG decode → PPA rotate/scale → DMA
to the DSI framebuffer`. It touches neither Slint nor LVGL, and the CPU never rotates a pixel. The
current firmware's software renderer plus CPU-side 270° rotation of a 2 MB RGB565 buffer
(`frame-ui/src/display.rs`, `frame_embedded_ui.c`) should be retired for photos.

---

## 4. Wi-Fi and Bluetooth: the C6 co-processor

The P4 has **no radio**. All Wi-Fi and BLE go through the ESP32-C6 over ESP-Hosted on SDIO.

### The shipped slave firmware supports Bluetooth

`JC-C6-slave_v2.3.2.bin` (and the older `JC8012P4A1_C6.bin`) contain, verifiably by string
extraction:

```
Supported features are:
  - WLAN over SDIO
  - HCI Over SDIO
Transport used :: SDIO only
```

plus `./main/slave_bt.c`, `esp_bt_controller_init`, `esp_bt_controller_enable(ESP_BT_MODE_BLE)`,
`hci_driver_vhci_host_tx`, and `hci_transport_init`.

**Confirmed on hardware 2026-08-25.** BLE works on this board without reflashing the C6. The
transport handshake reports `capabilities: 0xd` (`WLAN` + `HCI Over SDIO` + `BLE only`), a NimBLE
host on the P4 syncs with the controller on the C6, and the frame advertises, is discoverable from
an independent Bluetooth adapter at -43 dBm, and accepts connections — with Wi-Fi up at the same
time. The BLE address (`98:88:e0:…`) is the C6's own radio, distinct from the P4's Wi-Fi MAC.

**One caveat.** The board ships ESP-Hosted slave firmware **2.1.0** while the host component is
**2.12.0**:

```
W: Version mismatch: Host [2.12.0] > Co-proc [2.1.0] ==> Upgrade co-proc to avoid RPC timeouts
```

`esp_hosted_bt_controller_init()` / `..._enable()` are newer RPCs and return
`ESP_ERR_NOT_SUPPORTED`. Older slaves bring their BT controller up automatically at boot, so
treating that as non-fatal and going straight to `nimble_port_init()` works. Do not depend on those
RPCs. The vendor's `JC-C6-slave_v2.3.2.bin` is also older than 2.12, so flashing it would not close
the gap.

Note the C6's own UART is **not** wired to the P4 — it only reaches header CN5 — so HCI-over-UART
is not an option. SDIO is the only path.

### Host-side configuration (from the vendor's working demo)

```
CONFIG_ESP_HOSTED_ENABLED=y
CONFIG_ESP_HOSTED_SDIO_HOST_INTERFACE=y
CONFIG_ESP_HOSTED_SDIO_SLOT=1
CONFIG_ESP_HOSTED_SDIO_PIN_D0=14
CONFIG_ESP_HOSTED_SDIO_PIN_D1=15
CONFIG_ESP_HOSTED_SDIO_PIN_D2=16
CONFIG_ESP_HOSTED_SDIO_PIN_D3=17
CONFIG_ESP_HOSTED_SDIO_PIN_CLK=18
CONFIG_ESP_HOSTED_SDIO_PIN_CMD=19
CONFIG_ESP_HOSTED_SDIO_4_BIT_BUS=y
CONFIG_ESP_HOSTED_SDIO_CLOCK_FREQ_KHZ=40000
CONFIG_ESP_HOSTED_GPIO_SLAVE_RESET_SLAVE=54
CONFIG_ESP_HOSTED_SDIO_RESET_ACTIVE_HIGH=y
CONFIG_ESP_HOSTED_SDIO_RESET_DELAY_MS=1500
CONFIG_ESP_HOSTED_SLAVE_RESET_ON_EVERY_HOST_BOOTUP=y
CONFIG_ESP_HOSTED_IDF_SLAVE_TARGET="esp32c6"
CONFIG_ESP_WIFI_REMOTE_ENABLED=y
CONFIG_ESP_WIFI_REMOTE_LIBRARY_HOSTED=y
```

Component versions: `espressif/esp_hosted` 2.11.7 in the demo, 2.12.3 in this repo's lock;
`espressif/esp_wifi_remote` 1.2.*.

To bring up BLE, additionally enable `CONFIG_BT_ENABLED`, the NimBLE host, and ESP-Hosted's BT
transport. Every demo ships with `# CONFIG_BT_ENABLED is not set`, so **no vendor demo exercises
this path** — it is supported by the firmware but unproven in the vendor's own code.

### Reflashing the C6 (only if ever needed)

Header **CN5** carries `VCC3V3`, `GND`, `C6_U0TXD`, `C6_U0RXD`, `C6_IO9` (boot strap), and
`C6_CHIP_PU`. Capture the stock image before overwriting anything.

---

## 4a. Boot-time gotchas found on hardware

**The legacy I2C driver aborts the boot.** ESP-IDF's `driver` component always compiles the legacy
I2C driver, and at least one managed component still references it. The new `i2c_master` driver's
constructor detects both and calls `abort()`, producing a boot loop:

```
E (1273) i2c: CONFLICT! driver_ng is not allowed to be used with this old driver
abort() was called at PC 0x400fbd47 on core 0
```

Everything this firmware actually uses is on the new driver, so the two are never mixed at runtime.
Set `CONFIG_I2C_SKIP_LEGACY_CONFLICT_CHECK=y`.

**The partition CSV path cannot be relative.** ESP-IDF resolves
`CONFIG_PARTITION_TABLE_CUSTOM_FILENAME` against its own generated project directory, not the repo,
so the path must be absolute — and therefore cannot be committed. The Makefile generates
`target/sdkconfig.partition.generated` at build time instead.

**The panel driver is wrong in the upstream BSP** — see the next section. The frame boots and runs,
but the display does not receive a correct init sequence.

## 5. Power

```
USB-C 5V ──> IP5306 (charge + boost) ──> VOUT-BAT ──> TLV62569 ──> 3V3
                    │                                                │
              Li-ion CN1                                       TLV62569 ──> 1.2V (ESP_VDD_HP)
                                                                     (gated by EN_DCDC, pin 79)
```

- `3V3` = `VCC3V3` = `ESP_3V3` = `VDDA`, all one rail
- Internal P4 LDOs: **VO1** → flash, **VO3** → MIPI DPHY (2.5 V), **VO4** → SD card
- Battery: Li-ion via CN1, IP5306 charge/boost, level readable on **GPIO52**
- SW1 is the IP5306 `KEY` power button
- Nominal draw ~700 mA at 5 V

The frame is intended to run on mains. The battery path exists but is out of scope for this
project.

---

## 6. Divergences from the Espressif EV board

The vendor copied Espressif's `esp32_p4_function_ev_board` BSP. Most of it fits. These do not:

| Item | Espressif EV board | This board | Impact |
|---|---|---|---|
| **Touch controller** | GT911 | **GSL3680** | **The stock BSP's touch will not work.** The vendor ships a custom `esp_lcd_touch_gsl3680` component under `/source/.../common_components/`. This repo currently pulls the *upstream* BSP (`espressif/esp32_p4_function_ev_board` 5.2.3), so touch is broken until that driver is vendored in. |
| Panel | EK79007 1024×600 | **JD9365 800×1280** | Select `CONFIG_BSP_LCD_TYPE_1280_800` (already set in `sdkconfig.defaults`) |
| Camera | — | OV02C10 on CSI | Out of scope |
| RGB LED | none | WS2812 on GPIO26 | Free status indicator |

Because touch is at risk and needs a vendored driver, **prefer the GPIO35 BOOT button over a touch
gesture for the factory-reset interaction.** It removes a dependency from a
privacy-critical code path.

---

## 7. Corrections to `docs/SPECIFICATION.md`

That document was written from the vendor PDF and misread the **package pin number** column as GPIO
numbers in two places.

| Claim | Correct value | Why the error |
|---|---|---|
| "LCD Backlight — **GPIO25**" | **GPIO23** | GPIO23 sits on package pin **25** |
| "Touch Reset — **GPIO24**" | **GPIO22** | GPIO22 sits on package pin **24** |
| "Touch: I2C (`RTC_CLK/SCL1`, `RTC_DAT/SDA1`)" — implying a second bus | Same bus as the codec: **GPIO8 / GPIO7** | Those are alias names for GPIO8/GPIO7, per the codec sheet's net-alias table |

Verified correct in that document: LCD Reset GPIO27, Touch INT GPIO21, PA_CTRL GPIO20, the I2S
map (GPIO9-13), ES_I2C GPIO7/8, UART0 GPIO37/38, and the C6-on-SD2 description.

GPIO25 is `USB1P1_P` and GPIO24 is also USB-side — neither is anywhere near the backlight.

---

## 8. Quick config reference

```
# Panel
CONFIG_BSP_LCD_TYPE_1280_800=y

# PSRAM (32MB, hex mode)
CONFIG_SPIRAM=y
CONFIG_SPIRAM_MODE_HEX=y
CONFIG_SPIRAM_SPEED_200M=y
CONFIG_CACHE_L2_CACHE_256KB=y
CONFIG_CACHE_L2_CACHE_LINE_128B=y

# Flash
CONFIG_ESPTOOLPY_FLASHSIZE_16MB=y

# Silicon revision
CONFIG_ESP32P4_SELECTS_REV_LESS_V3=y
CONFIG_ESP32P4_REV_MIN_100=y
```

Flashing: USB-C via CH340C on UART0 (GPIO37/38). Hold **BOOT (SW3)** while tapping **RESET (SW2)**
to enter download mode. The vendor also ships `flash_download_tool_3.9.7` for Windows, which is not
needed here — `esptool` works.

---

## 9. What this means for the photo frame

1. **BLE provisioning is viable.** The shipped C6 firmware exposes HCI over SDIO. The spike becomes
   "enable and verify", not "find out whether it is possible at all".
2. **The photo pipeline should be pure hardware.** JPEG decoder plus PPA, bypassing Slint and LVGL,
   with Home Assistant delivering baseline JPEG at 1280×800.
3. **The SD card is unobstructed.** Dedicated 4-bit SDMMC bus, no contention with Wi-Fi; remember to
   power the `ESP_LDO_VO4` rail.
4. **Use the BOOT button for factory reset,** not a touch gesture — it is a real button and does not
   depend on the at-risk GSL3680 driver.
5. **Use the WS2812 LED for status.** Setup progress, connection state, and reset confirmation can
   be conveyed without ever putting developer text on the panel.
6. **Vendor the GSL3680 touch driver** from `/source/.../common_components/esp_lcd_touch_gsl3680/`
   if touch is wanted at all; the upstream BSP's GT911 will not talk to this panel.
