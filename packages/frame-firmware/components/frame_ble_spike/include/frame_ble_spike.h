#pragma once

#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Bring up a connectable BLE peripheral and start advertising.
 *
 * Spike for task T056: proves the ESP32-P4 can run a NimBLE host with the
 * BLE controller on the ESP32-C6, exchanging HCI over the ESP-Hosted SDIO
 * link. If this works, Improv Wi-Fi provisioning over BLE is viable and the
 * SoftAP fallback is not needed.
 *
 * Must be called after the Wi-Fi/ESP-Hosted link to the co-processor is up.
 *
 * @param device_name  Advertised name, NUL-terminated.
 */
esp_err_t frame_ble_spike_start(const char *device_name);

#ifdef __cplusplus
}
#endif
