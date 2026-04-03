# Hardware Specification: JC8012P4A1C_I_W

[Product Link](https://www.aliexpress.us/w/wholesale-JC8012P4A1C_I_W.html?spm=a2g0o.home.search.0)

**Manufacturer:** Shenzhen Jingcai Intelligent Co., Ltd [cite: 1, 2]
**Module Name:** 10.1 inch ESP32P4 module (JC8012P4A1C_I_W) [cite: 3, 4]

## 1. System Overview

This development board is built around the ESP32-P4 module as the main control unit, featuring a high-performance dual-core MCU capable of reaching 400MHz[cite: 14]. It integrates a high-resolution 10.1-inch display, capacitive touch, audio processing, and extensive peripheral connectivity tailored for advanced HMI (Human-Machine Interface) applications[cite: 14, 15].

### Core Processing & Memory

- **Main MCU:** ESP32-P4 (Dual-core RISC-V @ 400MHz) [cite: 14, 53]
- **SRAM / ROM:** 768 KB HP L2MEM, 32 KB LP SRAM, 128 KB HP ROM [cite: 14]
- **External RAM:** 32MB PSRAM [cite: 14]
- **Storage (Flash):** 16MB (W25Q128 3.3V) [cite: 14]
- **Wireless Co-Processor:** ESP32-C6 (Connected via SDIO/UART to provide Wi-Fi and Bluetooth capabilities) [cite: 14, 54]

## 2. Display Subsystem

The module features a military-grade 10.1-inch color screen designed for long-term stable operation[cite: 18, 28].

- **Panel Type:** 10.1-inch TFT IPS [cite: 34]
- **Resolution:** 800 x 1280 Pixels [cite: 20, 34]
- **Color Depth:** 24-bit RGB, 16.7M colors [cite: 18, 34]
- **Display Driver:** JD9365 [cite: 34]
- **Interface:** MIPI DSI (High-Speed)
- **Effective Display Area:** 216.58 mm \* 135.36 mm [cite: 34]
- **Backlight Control:** MP3202 Boost Converter (Controlled via PWM)

## 3. Touch Interface

- **Type:** Capacitive Touch Screen (SKU: JC8012P4A1C_I_W_Y / Y1) [cite: 34]
- **Interface:** I2C (`RTC_CLK/SCL1`, `RTC_DAT/SDA1`)
- **Connection:** FPC Connector (CTP-FPC) [cite: 60]

## 4. Audio Subsystem

- **Audio Codec:** ES8311 (I2C control, I2S data)
- **Speaker Amplifier:** NS4150 (with dedicated enable control)
- **Audio I/O:** \* On-board Microphone (MSM381A3729H9CP) [cite: 63]
  - Speaker Interface [cite: 63]

## 5. Connectivity & Peripherals

- **USB Ports:** Type-C interfaces supporting Full-Speed USB, High-Speed USB, and USB-TTL (via CH340C for UART flashing/debugging) [cite: 57, 62]
- **Storage Expansion:** TF Card slot (SD/MMC interface) [cite: 23, 58]
- **Camera:** CSI (Camera Serial Interface) FPC connector [cite: 61]
- **Real-Time Clock (RTC):** RX8025T-UC with dedicated battery holder [cite: 55]
- **Expand IO:** Header for additional GPIO and I2C expansion [cite: 16]

## 6. Power Specifications

- **Operating Voltage:** 5V (USB or External VIN) [cite: 34]
- **Power Consumption:** ~700mA [cite: 34]
- **Battery Support:** Lithium battery connector with IP5306 charging/boost management circuit [cite: 27, 34, 56]
- **Internal Power Rails:** TLV62569 step-down converters for 3.3V and 1.2V (ESP_VDD_HP) logic.

## 7. Mechanical & Environmental

- **Module Size (L x W):** 242.80 mm \* 158.70 mm [cite: 34, 67, 74]
- **Product Weight:** ~550g [cite: 34]
- **Operating Temperature:** -20°C to 70°C [cite: 34]
- **Storage Temperature:** -30°C to 80°C [cite: 34]

---

## 8. Crucial Pin Mapping / Hardware Routing Reference

### Display & Touch (ESP32-P4)

| Function            | Pin Name | Note                        |
| :------------------ | :------- | :-------------------------- |
| **LCD Reset**       | GPIO27   | Controls `LCD_RST` sequence |
| **LCD Backlight**   | GPIO25   | Controls `LCD_PWM`          |
| **Touch Interrupt** | GPIO21   | `TOUCH_INT`                 |
| **Touch Reset**     | GPIO24   | `TOUCH_RST`                 |

### Audio (ESP32-P4 -> ES8311 Codec)

| Function               | Pin Name | Note                       |
| :--------------------- | :------- | :------------------------- |
| **I2C SDA (Codec)**    | GPIO07   | `ES_I2C_SDA`               |
| **I2C SCL (Codec)**    | GPIO08   | `ES_I2C_SCL`               |
| **I2S DSDIN**          | GPIO09   | Data In                    |
| **I2S LRCK**           | GPIO10   | Left/Right Clock           |
| **I2S SDOUT**          | GPIO11   | Data Out                   |
| **I2S SCLK**           | GPIO12   | Serial Clock               |
| **I2S MCLK**           | GPIO13   | Master Clock               |
| **Speaker PA Control** | GPIO20   | `PA_CTRL` (Enables NS4150) |

### Communication Interconnects

| Function                   | Pin/Bus         | Note                                      |
| :------------------------- | :-------------- | :---------------------------------------- |
| **ESP32-C6 Co-Processor**  | SD2 Bus         | `SD2_CMD`, `SD2_CLK`, `SD2_D0` - `SD2_D3` |
| **USB D+/D- (High Speed)** | USB1P1_P / N    | Direct USB mapping                        |
| **UART0 TX/RX**            | GPIO37 / GPIO38 | CH340C Debug/Flash interface              |
